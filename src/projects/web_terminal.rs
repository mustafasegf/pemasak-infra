use std::{net::SocketAddr, time::Duration, borrow::Cow};

use axum::{extract::{WebSocketUpgrade, Path, ConnectInfo, ws::{Message, CloseFrame}, State}, TypedHeader, headers, response::{Response, IntoResponse}};
use bollard::{Docker, exec::{CreateExecOptions, StartExecResults}};
use futures_util::{StreamExt, SinkExt};
use hyper::{Body, StatusCode};
use leptos::{ssr::render_to_string, view, IntoView};
use tokio::io::AsyncWriteExt;
use serde::{Deserialize, Serialize};

use crate::{components::Base, startup::AppState, projects::components::ProjectHeader};

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsRequest {
    pub message: String,
    #[serde(rename = "HEADERS")]
    pub headers: Headers,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Headers {
    #[serde(rename = "HX-Request")]
    pub hx_request: Option<String>,
    #[serde(rename = "HX-Trigger")]
    pub hx_trigger: Option<String>,
    #[serde(rename = "HX-Trigger-Name")]
    pub hx_trigger_name: Option<String>,
    #[serde(rename = "HX-Target")]
    pub hx_target: Option<String>,
    #[serde(rename = "HX-Current-URL")]
    pub hx_current_url: Option<String>,
}

#[tracing::instrument]
pub async fn ws(
    Path((owner, project)): Path<(String, String)>,
    // State(AppState { pool, base, .. }): State<AppState>,
    ws: WebSocketUpgrade,
    user_agent: Option<TypedHeader<headers::UserAgent>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    let user_agent = if let Some(TypedHeader(user_agent)) = user_agent {
        user_agent.to_string()
    } else {
        String::from("Unknown browser")
    };

    // let who = SocketAddr::from(([127, 0, 0, 1], 0));
    let who = addr;

    tracing::info!(user_agent, "New websocket connection");

    ws.on_upgrade(move |mut socket| {
        async move {
            //send a ping (unsupported by some browsers) just to kick things off and get a response
            if socket.send(Message::Ping(vec![])).await.is_ok() {
                tracing::debug!(?who, "Pinged");
            } else {
                tracing::debug!(?who, "Could not send ping");
                return;
            }

            // receive single message from a client (we can either receive or send with socket).
            // this will likely be the Pong for our Ping or a hello message from client.
            // waiting for message from a client will block this task, but will not block other client's
            // connections.
            if let Some(msg) = socket.recv().await {
                if let Ok(msg) = msg {
                    if let Message::Close(c) = msg {
                        if let Some(cf) = c {
                            tracing::debug!(?who, code = cf.code, reason = ?cf.reason, "client disconected");
                        } else {
                            tracing::debug!(?who, "client disconected wihtout CloseFrame");
                        }
                    }
                } else {
                    println!("client {who} abruptly disconnected");
                    return;
                }
            }

            let docker = match Docker::connect_with_local_defaults() {
                Ok(docker) => docker,
                Err(err) => {
                    tracing::error!(?err, "Can't start terminal: Failed to connect to docker");
                    return;
                }
            };

            let container_name = format!("{owner}-{}", project.trim_end_matches(".git")).replace('.', "-");
            let exec = match docker
                .create_exec(
                    &container_name,
                    CreateExecOptions::<&str> {
                        attach_stdout: Some(true),
                        attach_stderr: Some(true),
                        attach_stdin: Some(true),
                        tty: Some(true),
                        cmd: Some(vec!["bash"]),
                        ..Default::default()
                    },
                )
                .await
            {
                Ok(exec) => exec,
                Err(err) => {
                    tracing::error!(?err, "Can't start terminal: Failed to create exec");
                    return;
                }
            };

            let (mut input, mut output)  =  match docker.start_exec(&exec.id, None).await {
                Ok(StartExecResults::Attached { output, input }) => (input , output),
                Ok(StartExecResults::Detached) => {
                    tracing::error!("Can't start terminal: Failed to start exec");
                    return;
                },
                Err(err) => {
                    tracing::error!(?err, "Can't start terminal: Failed to start exec");
                    return;
                }
            };

            // By splitting socket we can send and receive at the same time. In this example we will send
            let (mut sender, mut receiver) = socket.split();

            let mut send_task = tokio::spawn(async move {
                let mut i = 0;
                loop {

                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_secs(10)) => {
                            if sender.send(Message::Ping(vec![])).await.is_err() {
                                break;
                            }
                        },
                        msg = output.next() => {
                            match msg {
                                Some(Ok(output)) => {
                                    let bytes = output.clone().into_bytes();
                                    let bytes = strip_ansi_escapes::strip(&bytes);
                                    let msg = String::from_utf8_lossy(&bytes);

                                    if sender
                                        .send(Message::Text(format!(r#"<pre id="data" hx-swap-oob="beforeend">{msg}</pre> "#)))
                                        .await
                                        .is_err()
                                    {
                                        break;
                                    }
                                    i += 1;
                                },
                                Some(Err(err)) => {
                                    tracing::error!(?err, "Can't receive message from terminal");
                                    break;
                                },
                                None => {
                                    tracing::error!("Can't receive message from terminal");
                                    break;
                                }
                            }
                        },

                    }
                }

                tracing::debug!(?who, "Sending close");
                if let Err(e) = sender
                    .send(Message::Close(Some(CloseFrame {
                        code: axum::extract::ws::close_code::NORMAL,
                        reason: Cow::from("Goodbye"),
                    })))
                    .await
                {
                    tracing::debug!(?e, "Could not send Close due to {e}");
                }
                i
            });

            // This second task will receive messages from client
            let mut recv_task = tokio::spawn({
                async move {
                    let mut cnt = 0;
                    while let Some(Ok(msg)) = receiver.next().await {
                        cnt += 1;
                        // print message and break if instructed to do so
                        match msg {
                            Message::Text(t) => {
                                match serde_json::from_str::<WsRequest>(&t) {
                                    Err(err) => {
                                        tracing::debug!(?err, "Can't parse message");
                                    },
                                    Ok(msg) => {
                                        let mut msg = msg.message;
                                        msg.push_str("\n");
                                        match input.write_all(msg.as_bytes()).await {
                                            Err(err) => {
                                                tracing::error!(?err, "Can't write to terminal");
                                                break;
                                            },
                                            Ok(_) => {
                                                // if let Err(err) = tx.send(WsMessage::Message(msg.message)).await {
                                                //     tracing::error!(?err, "Can't send message");
                                                // }
                                            }
                                        }
                                    }
                                };
                            }
                            Message::Close(c) => {
                                if let Some(cf) = c {
                                    tracing::debug!(?who, code = cf.code, reason = ?cf.reason, "client disconected");
                                } else {
                                    tracing::debug!(?who, "client disconected wihtout CloseFrame");
                                }
                                break;
                            }
                            _ => {}

                        }
                    }
                    cnt
            }});


            // If any one of the tasks exit, abort the other.
            tokio::select! {
                rv_a = (&mut send_task) => {
                    match rv_a {
                        Ok(a) => println!("{a} messages sent to {who}"),
                        Err(a) => println!("Error sending messages {a:?}")
                    }
                    recv_task.abort();
                },
                rv_b = (&mut recv_task) => {
                    match rv_b {
                        Ok(b) => println!("Received {b} messages"),
                        Err(b) => println!("Error receiving messages {b:?}")
                    }
                    send_task.abort();
                },
            }

            // returning from the handler closes the websocket connection
            tracing::info!(?who, "Websocket context destroyed");
        }
    })
}

#[tracing::instrument]
pub async fn get(Path((owner, project)): Path<(String, String)>, State(AppState { domain, .. }): State<AppState>) -> Response<Body> {
    let ws_path = format!("/{owner}/{project}/terminal/ws");
    let html = render_to_string(move || {
        view! {
            <Base>
                <ProjectHeader owner={owner.clone()} project={project.clone()} domain={domain.clone()}></ProjectHeader>

                <h2 class="text-xl mb-4">
                    Web Terminal
                </h2>
                <div class="bg-neutral/40 backdrop-blur-sm p-2 mockup-code" hx-ext="ws" ws-connect={ws_path} hx-on="htmx:wsAfterMessage: const data = document.getElementById('data'); data.scrollTop = data.scrollHeight;"> 
                    <pre id="data" hx-swap-oob="beforeend" class="flex flex-col gap-1 px-4 max-h-64 overflow-y-auto"></pre>

                    <form class="px-4" id="form" ws-send hx-on="
                        htmx:wsBeforeSend: if (JSON.parse(event?.detail?.message).message === 'clear') document.getElementById('data').innerHTML = '';
                        htmx:wsAfterSend: this.reset();
                    ">
                        <div class="flex space-x-2">
                            <span>
                                {"> "}
                            </span>
                            <input id="terminal-input" name="message" type="text" placeholder="enter command" 
                            // class="input input-bordered w-full max-w-xs"
                                class="bg-transparent border-transparent w-full text-white !outline-none"
                            />
                        </div>
                    </form>
                </div>
                <pre id="result"></pre>
            </Base>
        }
    })
    .into_owned();

    Response::builder()
        .status(StatusCode::OK)
        .body(Body::from(html))
        .unwrap()
}
