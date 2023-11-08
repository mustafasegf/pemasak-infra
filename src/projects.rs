use std::{fs::File, collections::HashMap, net::SocketAddr, ops::ControlFlow, borrow::Cow};

use axum::{
    extract::{State, Path,ws::{Message, WebSocket, WebSocketUpgrade, CloseFrame}, ConnectInfo},
    response::{Response, IntoResponse},
    routing::{get, post},
    Form, Router, middleware, TypedHeader, headers,
};
use axum_extra::routing::RouterExt;
use bollard::{Docker, container::{StopContainerOptions, RemoveContainerOptions, StartContainerOptions}, network::InspectNetworkOptions};
use garde::{Validate, Unvalidated};
use hyper::{Body, StatusCode};
use leptos::{*, ssr::render_to_string};
use serde::Deserialize;
use ulid::Ulid;
use uuid::Uuid;

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Argon2,
};
use rand::{Rng, SeedableRng};

use crate::{startup::AppState, configuration::Settings, auth::{auth, Auth}, components::Base};
use futures::{sink::SinkExt, stream::StreamExt};

// Base64 url safe
const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
const TOKEN_LENGTH: usize = 32;


pub async fn router(_state: AppState, _config: &Settings) -> Router<AppState, Body> {
    Router::new()
        .route("/new", get(create_project_ui).post(create_project))
        .route("/dashboard", get(dashboard_ui).post(create_project))
        .route_with_tsr("/:owner/:project", get(project_ui))
        .route_with_tsr("/:owner/:project/delete", post(delete_project))
        .route_with_tsr("/:owner/:project/volume/delete", post(delete_volume))
        .route_layer(middleware::from_fn(auth))
        .route_with_tsr("/:owner/:project/terminal/ws", get(web_terminal_ws))
        
}

#[derive(Deserialize, Validate, Debug)]
pub struct CreateProjectRequest {
    #[garde(length(min=1))]
    pub owner: String,
    #[garde(alphanumeric)]
    pub project: String,
}

#[tracing::instrument(skip(pool, base, domain))]
pub async fn create_project(
    State(AppState { pool, base, domain, .. }): State<AppState>,
    Form(req): Form<Unvalidated<CreateProjectRequest>>,
) -> Response<Body> {

    let CreateProjectRequest{ owner, project } = match req.validate(&()){
        Ok(valid) => valid.into_inner(),
        Err(err) => {
            let html = render_to_string(move || { view! {
                <p> {err.to_string() } </p>
            }}).into_owned();
            return Response::builder().status(StatusCode::BAD_REQUEST).body(Body::from(html)).unwrap();
        }
    };

    let path = match project.ends_with(".git") {
        true => format!("{base}/{owner}/{project}"),
        false => format!("{base}/{owner}/{project}.git"),
    };

    // check if owner exist
    let owner_id = match sqlx::query!(
        r#"SELECT id FROM project_owners WHERE name = $1 AND deleted_at IS NULL"#,
        owner,
    ).fetch_optional(&pool).await {
        Ok(Some(data)) => data.id,
        Ok(None) => {
            let html = render_to_string(move || { view! {
                <h1> Owner does not exist </h1>
            }}).into_owned();
            return Response::builder().status(StatusCode::BAD_REQUEST).body(Body::from(html)).unwrap();
        }
        Err(err) => {
            tracing::error!(?err, "Can't get project_owners: Failed to query database");
            let html = render_to_string(move || { view! {
                <h1> Failed to query database {err.to_string() } </h1>
            }}).into_owned();
            return Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Body::from(html)).unwrap();
        }
    };

    // check if project already exist
    match sqlx::query!(
        r#"SELECT id FROM projects WHERE name = $1 AND owner_id = $2"#,
        project,
        owner_id,
    ).fetch_optional(&pool).await {
        Ok(None) => {},
        Ok(_) => {
            let html = render_to_string(move || { view! {
                <h1> Project already exist</h1>
            }}).into_owned();
            return Response::builder().status(StatusCode::CONFLICT).body(Body::from(html)).unwrap();

        },
        Err(err) => {
            tracing::error!(?err, "Can't get projects: Failed to query database");
            let html = render_to_string(move || { view! {
                <h1> Failed to query database {err.to_string() } </h1>
            }}).into_owned();
            return Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Body::from(html)).unwrap();
        }
    }

    // TODO: create this into a tx and rollback if failed to create git repo
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(err) => {
            tracing::error!(?err, "Can't insert user: Failed to begin transaction");
            let html = render_to_string(move || { view! {
                <h1> Failed to begin transaction {err.to_string() } </h1>
            }}).into_owned();

            return Response::builder().status(StatusCode::BAD_REQUEST).header("Content-Type", "text/html").body(Body::from(html)).unwrap();
        }
    };

    // create project
    let project_id = match sqlx::query!(
        r#"INSERT INTO projects (id, name, owner_id) VALUES ($1, $2, $3) RETURNING id"#,
        Uuid::from(Ulid::new()),
        project,
        owner_id,
    ).fetch_one(&mut *tx).await {
        Ok(data) => data.id,
        Err(err) => {
            tracing::error!(?err, "Can't insert projects: Failed to insert into database");
            if let Err(err) = tx.rollback().await {
                tracing::error!(?err, "Can't insert projects: Failed to rollback transaction");
            }

            let html = render_to_string(move || { view! {
                <h1> Failed to insert into database</h1>
            }}).into_owned();
            return Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Body::from(html)).unwrap();
        }
    };

    if let Err(err) =  git2::Repository::init_bare(path) {
        tracing::error!(?err, "Can't create project: Failed to create repo");
        let html = render_to_string(move || { view! {
            <h1> Failed to create project: {err.to_string() } </h1>
        }}).into_owned();
        return Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Body::from(html)).unwrap();
    }

    // generate token
    let mut rng = rand::rngs::StdRng::from_entropy();
    let token = (0..TOKEN_LENGTH).map(|_| {
        let idx = rng.gen_range(0..CHARSET.len());
        CHARSET[idx] as char
    }).collect::<String>();

    let salt = SaltString::generate(&mut OsRng);
    let hasher = Argon2::default();
    let hash = match hasher.hash_password(token.as_bytes(), &salt) {
        Ok(hash) => hash,
        Err(err) => {
            tracing::error!(?err, "Can't create project: Failed to hash token");
            let html = render_to_string(move || { view! {
                <h1> Failed to generate token {err.to_string() } </h1>
            }}).into_owned();
            return Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Body::from(html)).unwrap();
        }
    };

    if let Err(err) = sqlx::query!(
        "INSERT INTO api_token (id, project_id, token) VALUES ($1, $2, $3)",
        Uuid::from(Ulid::new()),
        project_id,
        hash.to_string(),
    ).execute(&mut *tx).await {
        tracing::error!(?err, "Can't insert api_token: Failed to insert into database");
        let html = render_to_string(move || { view! {
            <h1> Failed to insert into database {err.to_string() } </h1>
        }}).into_owned();
        return Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Body::from(html)).unwrap();
    };


    if let Err(err) = tx.commit().await {
        tracing::error!(?err, "Can't create project: Failed to commit transaction");
        let html = render_to_string(move || { view! {
            <h1> Failed to commit transaction {err.to_string() } </h1>
        }}).into_owned();
        return Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Body::from(html)).unwrap();
    }

    let html = render_to_string(move || { view! {
        <h1> Project created successfully  </h1>
        <div class="p-4 mb-4 bg-gray-800">
            <pre><code id="code"> 
                git remote add origin {format!(" http://{domain}/{owner}/{project}")} <br/>
                {"git push -u origin master"}
            </code></pre>
        </div>
        <button
            class="btn btn-outline btn-secondary mb-4"
            onclick="
                let lb = '\\n'
                if(navigator.userAgent.indexOf('Windows') != -1) {{
                  lb = '\\r\\n'
                }}

                let text = document.getElementById('code').getInnerHTML().replaceAll('<br>', lb)
                if ('clipboard' in window.navigator) {{
                    navigator.clipboard.writeText(text)
                }}
            "
        >
          Copy to clipboard 
        </button>

        <div class="p-4 mb-4 bg-gray-800">
            <pre><code> 
              project token: <span id="token">{token} </span>
            </code></pre>
        </div>
        <button
            class="btn btn-outline btn-secondary"
            onclick="
                let text = document.getElementById('token').innerText
                if ('clipboard' in window.navigator) {{
                    navigator.clipboard.writeText(text)
                }}"
        >
          Copy to clipboard 
        </button>
    }}).into_owned();

    Response::builder().status(StatusCode::OK).body(Body::from(html)).unwrap()
}

#[tracing::instrument(skip(auth, pool))]
pub async fn create_project_ui(
    auth: Auth,
    State(AppState { pool, .. }): State<AppState>,
) -> Response<Body> {
    let user = auth.current_user.unwrap();

    let owners = match sqlx::query!(
        r#"select o.id, o.name
           FROM project_owners o
           JOIN users_owners uo on uo.owner_id = o.id
           where uo.user_id = $1
           AND o.deleted_at is NULL
        "#,
        user.id

    ).fetch_all(&pool).await {
        Ok(data) => data,
        Err(err) => {
            tracing::error!(?err, "Can't get owners: Failed to query database");
            let html = render_to_string(move || {
                view! {
                    <h1> Failed to query database {err.to_string() } </h1>
                }
            }).into_owned();
            return Response::builder().status(500).body(Body::from(html)).unwrap();
        }
    };

    let html = render_to_string(move || view! {
        <Base>
            <form 
              hx-post="/new" 
              hx-trigger="submit"
              hx-target="#result"
              class="flex flex-col mb-4 gap-2"
            >
                <h1 class="text-2xl font-bold"> Create Project </h1>
                <h3 class="text-lg"> "login as " {user.username} </h3>
                <div class="flex flex-row gap-2">
                    <div class="form-control">
                      <label class="label">
                        <span class="label-text">Owner</span>
                      </label>
                        <select name="owner" class="select select-bordered w-full max-w-xs">
                            {owners.into_iter().map(|owner|{ view!{ 
                                <option>{owner.name}</option>
                            }}).collect::<Vec<_>>()}
                        </select>
                    </div>
                    <div class="form-control">
                      <label class="label">
                        <span class="label-text">Project</span>
                      </label>
                        <input type="text" name="project" required class="input input-bordered w-full max-w-xs"/>
                    </div>
                </div>
                <button class="mt-4 btn btn-primary w-full max-w-xs">Create Project</button>
            </form>
            <div id="result"></div>
        </Base>
    }).into_owned();
    Response::builder().status(StatusCode::OK).header("Content-Type", "text/html").body(Body::from(html)).unwrap()
}


#[tracing::instrument(skip(auth, pool))]
pub async fn dashboard_ui(
    auth: Auth,
    State(AppState { pool, .. }): State<AppState>,
) -> Response<Body> {
    let user = auth.current_user.unwrap();

    let projects = match sqlx::query!(
        r#"SELECT projects.name AS project, project_owners.name AS owner
           FROM projects
           JOIN project_owners ON projects.owner_id = project_owners.id
           JOIN users_owners ON project_owners.id = users_owners.owner_id
           JOIN users ON users_owners.user_id = users.id
           WHERE users.id = $1
        "#,
        user.id
    ).fetch_all(&pool).await {
        Ok(data) => data,
        Err(err) => {
            tracing::error!(?err, "Can't get projects: Failed to query database");
            let html = render_to_string(move || {
                view! {
                    <h1> "Failed to query database "{err.to_string() } </h1>
                }
            }).into_owned();
            return Response::builder().status(500).body(Body::from(html)).unwrap();
        }
    };

    let html = render_to_string(move || view! {
        <Base>
            <h1 class="text-2xl font-bold">Your Projects</h1>
            <h3 class="text-lg">"login as " {user.username}</h3>
            <div hx-boost="true" class="flex flex-col gap-4">

                {projects.into_iter().map(|record|{ view!{ 
                <div class="bg-neutral text-info py-4 px-8 w-full">
                    {let name = format!("{}/{}", record.owner, record.project);
                    view!{<a href={name.clone()} class="text-sm">{name}</a>}}
                </div>
                }}).collect::<Vec<_>>()}
                <a href="/new" class="mt-4 btn btn-primary w-full max-w-xs">Create Project</a>
            </div>
        </Base>
    }).into_owned();
    Response::builder().status(StatusCode::OK).header("Content-Type", "text/html").body(Body::from(html)).unwrap()
}

#[tracing::instrument(skip(auth, pool))]
pub async fn project_ui(
    auth: Auth,
    State(AppState { pool, .. }): State<AppState>,
    Path((owner, project)): Path<(String, String)>,
) -> Response<Body> {
    let _user = auth.current_user.unwrap();

    let delete_path = format!("/{owner}/{project}/delete");
    let volume_path = format!("/{owner}/{project}/volume/delete");

    // check if project exist
    let _project_rec = match sqlx::query!(
        r#"SELECT projects.name AS project, project_owners.name AS owner
           FROM projects
           JOIN project_owners ON projects.owner_id = project_owners.id
           JOIN users_owners ON project_owners.id = users_owners.owner_id
           AND projects.name = $1
           AND project_owners.name = $2
        "#,
        project,
        owner,
    ).fetch_optional(&pool).await {
        Ok(Some(record)) => record,
        Ok(None) => {
            let html = render_to_string(move || { view! {
                <Base>
                    <h1> Project does not exist </h1>
                </Base>
            }}).into_owned();
            return Response::builder().status(StatusCode::BAD_REQUEST).body(Body::from(html)).unwrap();
        },
        Err(err) => {
            tracing::error!(?err, "Can't get projects: Failed to query database");
            let html = render_to_string(move || { view! {
                <h1> "Failed to query database " {err.to_string() } </h1>
            }}).into_owned();
            return Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Body::from(html)).unwrap();

        }
    };

    let html = render_to_string(move || { view! {
        <Base>
          <h1> {owner}"/"{project} </h1>
          <button
            hx-post={delete_path}
            hx-trigger="click"
            class="btn btn-error mt-4 w-full max-w-xs"
          >Delete Project</button>

          <button
            hx-post={volume_path}
            hx-trigger="click"
            class="btn btn-error mt-4 w-full max-w-xs"
          >Delete Database</button>

          <div id="result"></div>

        </Base>
    }}).into_owned();

    Response::builder().status(StatusCode::OK).body(Body::from(html)).unwrap()
}

#[tracing::instrument(skip(pool, base))]
pub async fn delete_volume(
    Path((owner, project)): Path<(String, String)>,
    State(AppState { pool, base, .. }): State<AppState>,
) -> Response<Body> {

    let container_name = format!("{owner}-{}", project.trim_end_matches(".git")).replace('.', "-") ;
    let db_name = format!("{}-db", container_name);
    let volume_name = format!("{}-volume", container_name);


    let docker = match Docker::connect_with_local_defaults() {
        Ok(docker) => docker,
        Err(err) => {
            tracing::error!(?err, "Can't delete volume: Failed to connect to docker");
            // TODO: better message
            return Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Body::from("")).unwrap();

        },
    };


    let turned_on = match docker.inspect_container(&db_name, None).await {
        Ok(_) => {
            match docker.stop_container(&db_name, None::<StopContainerOptions>).await {
                Ok(_) => {
                    true
                }
                Err(err) => {
                    tracing::error!(?err, "Can't delete volume: Failed to stop db");
                    false
                },
            }
        },
        Err(err) => {
            tracing::debug!(?err, "Can't delete volume: db does not exist");
            false
        },
    };

    let status = match docker.inspect_volume(&volume_name).await {
        Ok(_) => {
            match docker.remove_volume(&volume_name, None).await {
                Ok(_) => {
                    "successfully deleted"
                },
                Err(err) => {
                    tracing::error!(?err, "Can't delete volume: Failed to delete volume");
                    "failed to delete: volume error"
                },
            }
        },
        Err(err) => {
            tracing::debug!(?err, "Can't delete volume: volume does not exist");
            "failed to delete: volume does not exist"
        },
    };

    if turned_on {
        match docker.start_container(&db_name, None::<StartContainerOptions<&str>>).await {
            Ok(_) => {},
            Err(err) => {
                tracing::error!(?err, "Can't delete volume: Failed to start db");
            },
        }
    }

    let html = render_to_string(move || { view! {
        <div>
            <h1> {status} </h1>
        </div>
    }}).into_owned();
    Response::builder().status(StatusCode::OK).body(Body::from(html)).unwrap()
}

#[tracing::instrument(skip(pool, base))]
pub async fn delete_project(
    Path((owner, project)): Path<(String, String)>,
    State(AppState { pool, base, .. }): State<AppState>,
) -> Response<Body> {

    fn to_response(status: HashMap<&'static str, &'static str>) -> Response<Body> {
        let success = status.iter().all(|(_, v)| *v == "successfully deleted");
        let el = match success {
            true => {
                view! {
                    <div>
                        <h1> "successfully deleted repo" </h1>
                    </div>
                }
            }
            false => {
               view! {
                   <div>
                   <h1> "some action failed" </h1>
                    {status.into_iter().map(|(k, v)|{ view!{ 
                        <h1> {k.to_string()} {v.to_string()} </h1>
                    }}).collect::<Vec<_>>() }
                   </div>
                }
            }
        };

        let html = render_to_string(move || { view! {
            {el}
            <script>
            r#"
                setTimeout(function() {
                    window.location.href = '/dashboard';
                }, 5000);  // 3000 milliseconds = 3 seconds
            "#
            </script>
        }}).into_owned();
        Response::builder().status(StatusCode::OK).body(Body::from(html)).unwrap()
    }

    let path = match project.ends_with(".git") {
        true => format!("{base}/{owner}/{project}"),
        false => format!("{base}/{owner}/{project}.git"),
    };
    //TODO: better error log

    let mut status: HashMap<&'static str, &'static str> = HashMap::new();

    // check if owner exist
    match sqlx::query!(
        r#"SELECT id FROM project_owners WHERE name = $1 AND deleted_at IS NULL"#,
        owner,
    ).fetch_optional(&pool).await {
        Ok(Some(data)) => {
            // check if project exist
            match sqlx::query!(
                r#"SELECT id FROM projects WHERE name = $1 AND owner_id = $2"#,
                project,
                data.id,
            ).fetch_optional(&pool).await {
                Ok(Some(_)) => {
                    match sqlx::query!("DELETE FROM projects WHERE name = $1 AND owner_id = $2", project, data.id)
                        .execute(&pool)
                        .await 
                    {
                        Ok(_) => {
                            status.insert("project", "successfully deleted");
                        },
                        Err(err) => {
                            tracing::error!(?err, "Can't delete project: Failed to delete project");
                            status.insert("project", "failed to delete: database error");
                        },
                    }
                },
                Err(err) => {
                    tracing::error!(?err, "Can't delete project: Failed to query database");
                    status.insert("project", "failed to delete: database error");
                }
                _ => {
                    status.insert("project", "failed to delete: project does not exist");
                }
            };
        },
        Ok(None) => {
            tracing::debug!("Can't delete project: Owner does not exist");
        }
        Err(err) => {
            tracing::error!(?err, "Can't get project_owners: Failed to query database");
        }
    }


    // check if repo exists
    match File::open(&path) {
        Err(err) => {
            tracing::debug!(?err, "Can't delete project: Repo does not exist");
            status.insert("repo", "failed to delete: repo does not exist");
        },
        Ok(_) => {
            match std::fs::remove_dir_all(&path) {
                Ok(_) => {
                    status.insert("repo", "successfully deleted");
                },
                Err(err) => {
                    tracing::error!(?err, "Can't delete project: Failed to delete repo");
                    status.insert("repo", "failed to delete: repo error");
                },
            }

        },
    };

    let container_name = format!("{owner}-{}", project.trim_end_matches(".git")).replace('.', "-") ;
    let db_name = format!("{}-db", container_name);
    let network_name = format!("{}-network", container_name);
    let volume_name = format!("{}-volume", container_name);

    let docker = match Docker::connect_with_local_defaults() {
        Err(err) => {
            tracing::error!(?err, "Can't delete project: Failed to connect to docker");
            status.insert("container", "failed to delete: docker error");
            return to_response(status);

        },
        Ok(docker) => docker,
    };

    // remove container
    match docker.inspect_container(&container_name, None).await {
        Ok(_) => {
            match docker.stop_container(&container_name, None::<StopContainerOptions>).await {
                Ok(_) => {
                    match docker.remove_container(&container_name, None::<RemoveContainerOptions>).await {
                        Ok(_) => {
                            status.insert("container", "successfully deleted");
                        },
                        Err(err) => {
                            tracing::error!(?err, "Can't delete project: Failed to delete container");
                            status.insert("container", "failed to delete: container error");
                        },
                    }
                },
                Err(err) => {
                    tracing::error!(?err, "Can't delete project: Failed to stop container");
                    status.insert("container", "failed to delete: container error");
                }
            };
        },
        Err(err) => {
            tracing::debug!(?err, "Can't delete project: Container does not exist");
            status.insert("container", "failed to delete: container does not exist");
        },
    };


    // remove image
    match docker.inspect_image(&container_name).await {
        Ok(_) => {
            match docker.remove_image(&container_name, None, None).await {
                Ok(_) => {
                    status.insert("image", "successfully deleted");
                },
                Err(err) => {
                    tracing::error!(?err, "Can't delete project: Failed to delete image");
                    status.insert("image", "failed to delete: image error");
                },
            }
        },
        Err(err) => {
            tracing::debug!(?err, "Can't delete project: Image does not exist");
            status.insert("image", "failed to delete: image does not exist");
        },
    };

    
    // remove database
    match docker.inspect_container(&db_name, None).await {
        Ok(_) => {
            match docker.stop_container(&db_name, None::<StopContainerOptions>).await {
                Ok(_) => {
                    match docker.remove_container(&db_name, None::<RemoveContainerOptions>).await {
                        Ok(_) => {
                            status.insert("db", "successfully deleted");
                        },
                        Err(err) => {
                            tracing::error!(?err, "Can't delete project: Failed to delete db");
                            status.insert("db", "failed to delete: container error");
                        },
                    }
                },
                Err(err) => {
                    tracing::error!(?err, "Can't delete project: Failed to stop db");
                    status.insert("db", "failed to delete: container error");
                }
            };
        },
        Err(err) => {
            tracing::debug!(?err, "Can't delete project: db does not exist");
            status.insert("db", "failed to delete: container does not exist");
        },
    };

    // delete volume
    match docker.inspect_volume(&volume_name).await {
        Ok(_) => {
            match docker.remove_volume(&volume_name, None).await {
                Ok(_) => {
                    status.insert("volume", "successfully deleted");
                },
                Err(err) => {
                    tracing::error!(?err, "Can't delete project: Failed to delete volume");
                    status.insert("volume", "failed to delete: volume error");
                },
            }
        },
        Err(err) => {
            tracing::debug!(?err, "Can't delete project: volume does not exist");
            status.insert("volume", "failed to delete: volume does not exist");
        },
    };

    // remove network
    match docker.inspect_network(&network_name, Some(InspectNetworkOptions::<&str>{
        verbose: true,
        ..Default::default()
    })).await {
        Ok(_) => {
            match docker.remove_network(&network_name).await {
                Ok(_) => {
                    status.insert("network", "successfully deleted");
                },
                Err(err) => {
                    tracing::error!(?err, "Can't delete project: Failed to delete network");
                    status.insert("network", "failed to delete: network error");
                },
            }

        },
        Err(err) => {
            tracing::debug!(?err, "Can't delete project: network does not exist");
            status.insert("network", "failed to delete: network does not exist");
        },
    };

    to_response(status)
}

#[tracing::instrument]
pub async fn web_terminal_ws(
    Path((owner, project)): Path<(String, String)>,
    // State(AppState { pool, base, .. }): State<AppState>,
    ws: WebSocketUpgrade,
    user_agent: Option<TypedHeader<headers::UserAgent>>,
    // ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    let user_agent = if let Some(TypedHeader(user_agent)) = user_agent {
        user_agent.to_string()
    } else {
        String::from("Unknown browser")
    };

    let who = SocketAddr::from(([127, 0, 0, 1], 0));

    fn process_message(msg: Message, who: SocketAddr) -> ControlFlow<(), ()> {
        match msg {
            Message::Text(t) => {
                println!(">>> {who} sent str: {t:?}");
            }
            Message::Binary(d) => {
                println!(">>> {} sent {} bytes: {:?}", who, d.len(), d);
            }
            Message::Close(c) => {
                if let Some(cf) = c {
                    println!(
                        ">>> {} sent close with code {} and reason `{}`",
                        who, cf.code, cf.reason
                    );
                } else {
                    println!(">>> {who} somehow sent close message without CloseFrame");
                }
                return ControlFlow::Break(());
            }

            Message::Pong(v) => {
                println!(">>> {who} sent pong with {v:?}");
            }
            // You should never need to manually handle Message::Ping, as axum's websocket library
            // will do so for you automagically by replying with Pong and copying the v according to
            // spec. But if you need the contents of the pings you can see them here.
            Message::Ping(v) => {
                println!(">>> {who} sent ping with {v:?}");
            }
        }
        ControlFlow::Continue(())
    }

    tracing::info!(?user_agent, "New websocket connection");
    ws.on_upgrade(move |mut socket| {
        async move {
            // match socket.send(Message::Ping(vec![1,2,3])).await {
            //     Ok(_) => {
            //         tracing::info!("Ping sent to {}", addr);
            //     },
            //     Err(err) => {
            //         tracing::error!(?err, "Failed to send ping to {}", addr);
            //         return;
            //     }
            // }

                //send a ping (unsupported by some browsers) just to kick things off and get a response
    if socket.send(Message::Ping(vec![1, 2, 3])).await.is_ok() {
        println!("Pinged {who}...");
    } else {
        println!("Could not send ping {who}!");
        // no Error here since the only thing we can do is to close the connection.
        // If we can not send messages, there is no way to salvage the statemachine anyway.
        return;
    }

    // receive single message from a client (we can either receive or send with socket).
    // this will likely be the Pong for our Ping or a hello message from client.
    // waiting for message from a client will block this task, but will not block other client's
    // connections.
    if let Some(msg) = socket.recv().await {
        if let Ok(msg) = msg {
            if process_message(msg, who).is_break() {
                return;
            }
        } else {
            println!("client {who} abruptly disconnected");
            return;
        }
    }

    // Since each client gets individual statemachine, we can pause handling
    // when necessary to wait for some external event (in this case illustrated by sleeping).
    // Waiting for this client to finish getting its greetings does not prevent other clients from
    // connecting to server and receiving their greetings.
    for i in 1..5 {
        if socket
            .send(Message::Text(format!("Hi {i} times!")))
            .await
            .is_err()
        {
            println!("client {who} abruptly disconnected");
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    // By splitting socket we can send and receive at the same time. In this example we will send
    // unsolicited messages to client based on some sort of server's internal event (i.e .timer).
    let (mut sender, mut receiver) = socket.split();

    // Spawn a task that will push several messages to the client (does not matter what client does)
    let mut send_task = tokio::spawn(async move {
        let n_msg = 20;
        for i in 0..n_msg {
            // In case of any websocket error, we exit.
            if sender
                .send(Message::Text(format!("Server message {i} ...")))
                .await
                .is_err()
            {
                return i;
            }

            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        }

        println!("Sending close to {who}...");
        if let Err(e) = sender
            .send(Message::Close(Some(CloseFrame {
                code: axum::extract::ws::close_code::NORMAL,
                reason: Cow::from("Goodbye"),
            })))
            .await
        {
            println!("Could not send Close due to {e}, probably it is ok?");
        }
        n_msg
    });

    // This second task will receive messages from client and print them on server console
    let mut recv_task = tokio::spawn(async move {
        let mut cnt = 0;
        while let Some(Ok(msg)) = receiver.next().await {
            cnt += 1;
            // print message and break if instructed to do so
            if process_message(msg, who).is_break() {
                break;
            }
        }
        cnt
    });

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
        }
    }

    // returning from the handler closes the websocket connection
    println!("Websocket context {who} destroyed");
        }
    })

}


