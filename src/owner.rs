use axum::{
    extract::{Path, State},
    middleware,
    response::Response,
    routing::{get, post},
    Form, Router,
};
use axum_extra::routing::RouterExt;
use garde::{Unvalidated, Validate};
use hyper::{Body, StatusCode};
use leptos::ssr::render_to_string;
use leptos::*;
use serde::Deserialize;
use ulid::Ulid;
use uuid::Uuid;

use crate::{
    auth::{auth, Auth},
    components::Base,
    configuration::Settings,
    startup::AppState,
};

// TODO: separate schema for create and update when needed later on
#[derive(Deserialize, Validate, Debug)]
pub struct OwnerRequest {
    #[garde(length(max = 128))]
    pub name: String,
}

#[derive(Deserialize, Validate, Debug)]
pub struct UserSuggestionRequest {
    #[garde(length(min = 1))]
    pub username: String,
}

#[derive(Deserialize, Validate, Debug)]
pub struct InviteRequest {
    #[garde(required)]
    pub owner_id: Option<Uuid>,
    #[garde(required)]
    pub username: Option<String>,
}

#[tracing::instrument(skip(auth, pool))]
pub async fn remove_project_member(
    auth: Auth,
    State(AppState { pool, .. }): State<AppState>,
    Path((owner_id, user_id)): Path<(Uuid, Uuid)>,
) -> Response<Body> {
    let authed_user_id = auth.id;

    // Check if requesting user is already in owner group
    match sqlx::query!(
        r#"SELECT user_id, owner_id FROM users_owners
        WHERE user_id = $1 AND owner_id = $2
        "#,
        authed_user_id,
        owner_id
    )
    .fetch_optional(&pool)
    .await
    {
        Ok(Some(_)) => (),
        Ok(None) => {
            tracing::error!(
                "Can't find existing user_owner with user_id {} and owner_id {}",
                user_id,
                owner_id,
            );

            return Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .body(Body::empty())
                .unwrap();
        }
        Err(err) => {
            tracing::error!(
                ?err,
                "Can't get existing user_owner: Failed to query database"
            );

            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::empty())
                .unwrap();
        }
    }

    if let Err(err) = sqlx::query!(
        r#"DELETE FROM users_owners
        WHERE user_id = $1 AND owner_id = $2"#,
        user_id,
        owner_id
    )
    .execute(&pool)
    .await
    {
        tracing::error!(
            ?err,
            "Can't delete users_owners: Failed to insert into database"
        );

        let html = render_to_string(move || {
            view! {
                <h1> Failed to remove owner group member </h1>
            }
        })
        .into_owned();

        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .header("Content-Type", "text/html")
            .body(Body::from(html))
            .unwrap();
    };

    Response::builder()
        .status(StatusCode::NO_CONTENT)
        .body(Body::empty())
        .unwrap()
}

#[tracing::instrument(skip(auth, pool))]
pub async fn invite_project_member(
    auth: Auth,
    State(AppState { pool, .. }): State<AppState>,
    Form(req): Form<Unvalidated<InviteRequest>>,
) -> Response<Body> {
    let authed_user_id = auth.id;
    let validated_request = match req.validate(&()) {
        Ok(validated_request) => validated_request.into_inner(),
        Err(_err) => {
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from("Invalid request"))
                .unwrap();
        }
    };

    let owner_id = validated_request.owner_id.unwrap();
    let invited_username = validated_request.username.unwrap();

    // Check if requesting user is already in owner group
    match sqlx::query!(
        r#"SELECT user_id, owner_id FROM users_owners
        WHERE user_id = $1 AND owner_id = $2
        "#,
        authed_user_id,
        owner_id,
    )
    .fetch_optional(&pool)
    .await
    {
        Ok(Some(_)) => (),
        Ok(None) => {
            tracing::error!(
                "Can't find existing user_owner with user_id {} and owner_id {}",
                authed_user_id,
                owner_id,
            );

            return Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .body(Body::empty())
                .unwrap();
        }
        Err(err) => {
            tracing::error!(
                ?err,
                "Can't get existing user_owner: Failed to query database"
            );

            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::empty())
                .unwrap();
        }
    }

    let invited_user = match sqlx::query!(
        r#"SELECT id FROM users WHERE username = $1"#,
        invited_username
    )
    .fetch_optional(&pool)
    .await
    {
        Ok(Some(invited_user)) => invited_user.id,
        Ok(None) => {
            tracing::error!(
                "Can't get existing user: User not found with username {}",
                invited_username
            );

            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from(format!(
                    "User not found with username {}",
                    invited_username
                )))
                .unwrap();
        }
        Err(err) => {
            tracing::error!(?err, "Can't get existing user: Failed to query database",);

            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::empty())
                .unwrap();
        }
    };

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(err) => {
            tracing::error!(
                ?err,
                "Can't insert users_owners: Failed to begin transaction"
            );
            let html = render_to_string(move || {
                view! {
                    <h1> Failed to begin transaction {err.to_string()} </h1>
                }
            })
            .into_owned();

            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header("Content-Type", "text/html")
                .body(Body::from(html))
                .unwrap();
        }
    };

    if let Err(err) = sqlx::query!(
        r#"INSERT INTO users_owners (user_id, owner_id)
        VALUES ($1, $2)"#,
        invited_user,
        owner_id,
    )
    .execute(&mut *tx)
    .await
    {
        tracing::error!(
            ?err,
            "Can't insert users_owners: Failed to insert into database"
        );

        let error_html = match err {
            sqlx::Error::Database(err) => {
                if err.constraint() == Some("users_owners_pkey") {
                    render_to_string(move || {
                        view! {
                            <h1> User is already in the owner group </h1>
                        }
                    })
                    .into_owned()
                } else {
                    render_to_string(move || {
                        view! {
                            <h1> Failed to invite member to owner group </h1>
                        }
                    })
                    .into_owned()
                }
            }
            _ => render_to_string(move || {
                view! {
                    <h1> Failed to invite member to owner group </h1>
                }
            })
            .into_owned(),
        };

        if let Err(err) = tx.rollback().await {
            tracing::error!(
                ?err,
                "Can't insert users_owners: Failed to rollback transaction"
            );
        }

        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .header("Content-Type", "text/html")
            .body(Body::from(error_html))
            .unwrap();
    }

    if let Err(err) = tx.commit().await {
        tracing::error!(
            ?err,
            "Can't create users_owners: Failed to commit transaction"
        );
        let html = render_to_string(move || {
            view! {
                <h1> Failed to commit transaction {err.to_string() } </h1>
            }
        })
        .into_owned();
        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from(html))
            .unwrap();
    }

    Response::builder()
        .status(StatusCode::NO_CONTENT)
        .body(Body::empty())
        .unwrap()
}

#[tracing::instrument(skip(_auth, pool))]
pub async fn create_project_owner(
    _auth: Auth,
    State(AppState { pool, .. }): State<AppState>,
    Form(req): Form<Unvalidated<OwnerRequest>>,
) -> Response<Body> {
    let data = match req.validate(&()) {
        Ok(valid) => valid.into_inner(),
        Err(err) => {
            let html = render_to_string(move || {
                view! {
                    <p> {err.to_string() } </p>
                }
            })
            .into_owned();
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from(html))
                .unwrap();
        }
    };

    // Check for existing project
    match sqlx::query!(
        r#"SELECT id FROM project_owners
        WHERE name = $1
        "#,
        data.name
    )
    .fetch_optional(&pool)
    .await
    {
        Ok(None) => (),
        Ok(Some(_)) => {
            tracing::error!(
                "Project owner already exists with the following name: {}",
                data.name
            );

            let html = render_to_string(move || {
                view! {
                    <p> Project with name {data.name} already exists </p>
                }
            })
            .into_owned();

            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from(html))
                .unwrap();
        }
        Err(err) => {
            tracing::error!(
                ?err,
                "Can't get existing project owner: Failed to query database"
            );

            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::empty())
                .unwrap();
        }
    };

    let owner_id = Uuid::from(Ulid::new());

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(err) => {
            tracing::error!(
                ?err,
                "Can't insert project owner: Failed to begin transaction"
            );
            let html = render_to_string(move || {
                view! {
                    <h1> Failed to begin transaction {err.to_string()} </h1>
                }
            })
            .into_owned();

            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header("Content-Type", "text/html")
                .body(Body::from(html))
                .unwrap();
        }
    };

    if let Err(err) = sqlx::query!(
        r#"INSERT INTO project_owners (id, name)
        VALUES ($1, $2)
        "#,
        owner_id,
        data.name
    )
    .execute(&mut *tx)
    .await
    {
        tracing::error!(
            ?err,
            "Can't insert project owner: Failed to insert into database"
        );
        if let Err(err) = tx.rollback().await {
            tracing::error!(
                ?err,
                "Can't insert project owner: Failed to rollback transaction"
            );
        }

        let html = render_to_string(move || {
            view! {
                <h1> Failed to insert project owner into database </h1>
            }
        })
        .into_owned();

        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .header("Content-Type", "text/html")
            .body(Body::from(html))
            .unwrap();
    }

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/html")
        .body(Body::empty())
        .unwrap()
}

#[tracing::instrument()]
pub async fn update_project_owner(
    auth: Auth,
    State(AppState { pool, .. }): State<AppState>,
    Path(owner_id): Path<String>,
    Form(req): Form<Unvalidated<OwnerRequest>>,
) -> Response<Body> {
    Response::builder()
        .status(StatusCode::NO_CONTENT)
        .body(Body::empty())
        .unwrap()
}

pub async fn project_owner_suggestions(
    State(AppState { pool, .. }): State<AppState>,
    Form(req): Form<Unvalidated<UserSuggestionRequest>>,
) -> Response<Body> {
    let validated = match req.validate(&()) {
        Ok(validated) => validated.into_inner(),
        Err(_err) => {
            let html = render_to_string(move || {
                view! {
                    <ul id="user-suggestions" class="p-2 shadow menu dropdown-content z-[1] bg-base-100 rounded-box w-52"></ul>
                }
            }).into_owned();

            return Response::builder()
                .header("Content-Type", "text/html")
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from(html))
                .unwrap();
        }
    };

    let suggestions = match sqlx::query!(
        r#"SELECT username FROM users u WHERE username LIKE $1 LIMIT 5"#,
        format!("{}%", validated.username)
    )
    .fetch_all(&pool)
    .await
    {
        Ok(suggestions) => suggestions,
        Err(err) => {
            tracing::error!(
                ?err,
                "Failed to fetch user suggestions: failed to query database"
            );

            let html = render_to_string(|| {
                view! {
                    <ul id="user-suggestions" class="p-2 shadow menu dropdown-content z-[1] bg-base-100 rounded-box w-52"></ul>
                }
            }).into_owned();

            return Response::builder()
                .header("Content-Type", "text/html")
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from(html))
                .unwrap();
        }
    };

    let html = render_to_string(move || {
        // TODO(CRITICAL): Check if this can be SQL injected or not
        view! {
            <ul id="user-suggestions" class="p-2 shadow menu dropdown-content z-[1] bg-base-100 rounded-box w-52">
                {suggestions.into_iter().map(|record| {
                    let username = record.username;

                    view!{ 
                        <li _={format!(
                            "on click 
                            set #user-input @value to '{0}'
                            then set #user-input.value to '{0}' 
                            then set #user-suggestions.innerHTML to ''",
                            &username
                        )}>
                            <a>{username}</a>
                        </li>
                    }
                }).collect::<Vec<_>>()}
            </ul>
        }
    }).into_owned();

    Response::builder()
        .header("Content-Type", "text/html")
        .status(StatusCode::OK)
        .body(Body::from(html))
        .unwrap()
}

pub async fn project_owner_invite_member_ui(
    auth: Auth,
    State(AppState { pool, .. }): State<AppState>,
    Path(owner_id): Path<String>,
) -> Response<Body> {
    let auth_id = auth.id;
    let owner_id = match Uuid::parse_str(&owner_id) {
        Ok(owner_id) => owner_id,
        Err(err) => {
            tracing::error!(
                ?err,
                "Failed to fetch project owner: owner with id {} is not found (Invalid UUID)",
                owner_id
            );

            let html = render_to_string(|| {
                view! {
                    <div>
                        <h1 class="font-bold text-xl">Owner Group not found</h1>
                        <h2>Please ensure that you have permission to access the Owner Group.</h2>
                        <a href="/owner">
                            <button class="btn btn-neutral">Back To Owner Group List</button>
                        </a>
                    </div>
                }
            })
            .into_owned();

            return Response::builder()
                .header("Content-Type", "text/html")
                .body(Body::from(html))
                .unwrap();
        }
    };

    let project_owner = match sqlx::query!(
        r#"SELECT id, name FROM project_owners AS po
        LEFT JOIN users_owners AS uo ON po.id = uo.owner_id
        WHERE po.id = $1 AND uo.user_id = $2"#,
        owner_id,
        auth_id,
    )
    .fetch_optional(&pool)
    .await
    {
        Ok(Some(project_owner)) => project_owner,
        Ok(None) => {
            tracing::error!(
                "Failed to fetch project owner: owner with id {} and user id {} is not found",
                owner_id,
                auth_id
            );

            let html = render_to_string(|| {
                view! {
                    <div>
                        <h1 class="font-bold text-xl">Owner Group not found</h1>
                        <h2>Please ensure that you have permission to access the Owner Group.</h2>
                    </div>
                }
            })
            .into_owned();

            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .header("Content-Type", "text/html")
                .body(Body::from(html))
                .unwrap();
        }
        Err(err) => {
            tracing::error!(
                ?err,
                "Failed to fetch project owner: failed to fetch from database"
            );

            let html = render_to_string(|| {
                view! {
                    <div>
                        <h1 class="font-bold text-xl">An internal server error occurred</h1>
                        <h2>Please try again later</h2>
                    </div>
                }
            })
            .into_owned();

            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header("Content-Type", "text/html")
                .body(Body::from(html))
                .unwrap();
        }
    };

    let html = render_to_string(move || {
        view! {
            <div class="modal-box">
                <h3 class="font-bold text-lg">Invite Member</h3>
                <p class="py-4">
                    Enter the username you want to invite below
                </p>

                <form hx-post={format!("/owner/{owner_id}/invite")}>
                    <div hx-target="#user-suggestions">
                        <div class="dropdown dropdown-open w-full">
                            <input
                                type="hidden"
                                name="owner_id"
                                value={format!("{}", project_owner.id)}
                            >
                            </input>
                            <input
                                id="user-input"
                                name="username"
                                hx-post="/user/suggestions"
                                hx-trigger="keyup changed delay:500ms, username"
                                placeholder="Type here" 
                                class="input input-bordered w-full"
                            ></input>
                            <ul hx-swap="outerHTML" id="user-suggestions" class="p-2 shadow menu dropdown-content z-[1] bg-base-100 rounded-box w-52">
                            </ul>
                        </div>
                    </div>

                    <div class="modal-action">
                        <div class="w-full flex justify-between">
                            <button
                                type="button"
                                class="btn"
                                hx-on:click="document.getElementById('invite-modal').close()"
                            >
                                Close
                            </button>
                            <button type="submit" class="btn btn-primary">Invite</button>
                        </div>
                    </div>
                </form>
            </div>
        }
    }).into_owned();

    Response::builder()
        .status(StatusCode::OK)
        .body(Body::from(html))
        .unwrap()
}

pub async fn project_owner_group_details_ui(
    State(AppState { pool, .. }): State<AppState>,
    Path(owner_id): Path<String>,
) -> Response<Body> {
    let parsed_id = match Uuid::parse_str(&owner_id) {
        Ok(parsed_id) => parsed_id,
        Err(err) => {
            tracing::error!(
                ?err,
                "Failed to fetch project owner: project with id {} is not found (Invalid UUID)",
                owner_id
            );

            let html = render_to_string(|| {
                view! {
                    <Base is_logged_in={true}>
                        <h1 class="font-bold text-xl">Owner Group not found</h1>
                        <h2>Please ensure that you have permission to access the Owner Group.</h2>
                        <a href="/owner">
                            <button class="btn btn-neutral">Back To Owner Group List</button>
                        </a>
                    </Base>
                }
            })
            .into_owned();

            return Response::builder()
                .header("Content-Type", "text/html")
                .status(StatusCode::NOT_FOUND)
                .body(Body::from(html))
                .unwrap();
        }
    };

    let owner_group = match sqlx::query!(
        r#"SELECT po.id, po.name, po.created_at FROM project_owners AS po WHERE id = $1"#,
        parsed_id,
    )
    .fetch_optional(&pool)
    .await
    {
        Ok(Some(owner_group)) => owner_group,
        Ok(None) => {
            tracing::error!(
                "Failed to fetch project owner: project with id {} is not found",
                parsed_id
            );

            let html = render_to_string(|| {
                view! {
                    <Base is_logged_in={true}>
                        <h1 class="font-bold text-xl">Owner Group not found</h1>
                        <h2>Please ensure that you have permission to access the Owner Group.</h2>
                        <a href="/owner">Go Back To Owner Dashboard</a>
                    </Base>
                }
            })
            .into_owned();

            return Response::builder()
                .header("Content-Type", "text/html")
                .status(StatusCode::NOT_FOUND)
                .body(Body::from(html))
                .unwrap();
        }
        Err(err) => {
            tracing::error!(
                ?err,
                "Failed to fetch project owner: Failed to query database"
            );

            return Response::builder()
                .header("Content-Type", "text/html")
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::empty())
                .unwrap();
        }
    };

    let group_members = match sqlx::query!(
        r#"SELECT u.id, u.username, u.name FROM users AS u
        LEFT JOIN users_owners as uo ON uo.user_id = u.id AND uo.owner_id = $1
        WHERE u.id = uo.user_id"#,
        owner_group.id,
    )
    .fetch_all(&pool)
    .await
    {
        Ok(group_members) => group_members,
        Err(err) => {
            tracing::error!(
                ?err,
                "Failed to query group members: Failed to query database"
            );

            let html = render_to_string(|| {
                view! {
                    <Base is_logged_in={true}>
                        <h1 class="text-2xl">An error occurred while fetching this Owner Group</h1>
                        <h2 class="text-base-content">Please try again later.</h2>
                    </Base>
                }
            })
            .into_owned();

            return Response::builder()
                .header("Content-Type", "text/html")
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from(html))
                .unwrap();
        }
    };

    let html = render_to_string(move || {
        let name = owner_group.name;
        let id = owner_group.id;

        view! {
            <Base is_logged_in={true}>
                <div class="flex justify-between items-center bg-neutral rounded-lg p-8 mb-4">
                    <div>
                        <h1 class="text-3xl font-bold">{name}</h1>
                        <h2 class="text-neutral-content">{"ID: "}{id.to_string()}</h2>
                    </div>
                    <div>
                        <button class="btn btn-primary">Edit Group</button>
                    </div>
                </div>

                <div class="bg-neutral p-8 rounded-lg mb-4 space-y-4">
                    <div class="flex justify-between items-center">
                        <h1 class="text-2xl font-bold">Group Members</h1>
                        <button 
                            class="btn btn-primary" 
                            hx-get={format!("/owner/{}/invite", owner_id)} hx-target="#invite-modal"
                            hx-on:click="document.getElementById('invite-modal').showModal()"
                        >
                            Invite Member
                        </button>
                    </div>
                    <hr class="bg-base-content border-base-content"></hr>
                    <ul class="divide-y divide-base-content">
                        {group_members.into_iter().map(|record|{ 
                            let id = record.id;
                            let name = record.username;

                            view!{ 
                                <li>
                                    <div class="flex justify-between items-center py-4">
                                        <div>
                                            <h1 class="font-bold">{name}</h1>
                                            <h2 class="text-base-content">{"UID: "}{id.to_string()}</h2>
                                        </div>
                                        <div class="flex items-center">
                                            <button class="btn btn-outline btn-error">
                                                <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-6 h-6">
                                                    <path stroke-linecap="round" stroke-linejoin="round" d="M14.74 9l-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 01-2.244 2.077H8.084a2.25 2.25 0 01-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 00-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 013.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 00-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 00-7.5 0" />
                                                </svg>
                                            </button>
                                        </div>
                                    </div>
                                </li>
                            }
                        }).collect::<Vec<_>>()}
                    </ul>
                </div>
                <dialog id="invite-modal" class="modal">
                </dialog>
            </Base>
        }
    })
    .into_owned();

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/html")
        .body(Body::from(html))
        .unwrap()
}

pub async fn project_owner_group_list_ui(
    auth: Auth,
    State(AppState { pool, .. }): State<AppState>,
) -> Response<Body> {
    let user_id = auth.id;

    let owner_groups = match sqlx::query!(
        r#"SELECT po.id, po.name, po.created_at FROM project_owners AS po
        LEFT JOIN users_owners AS uo ON uo.owner_id = po.id
        WHERE uo.user_id = $1"#,
        user_id
    )
    .fetch_all(&pool)
    .await
    {
        Ok(owner_groups) => owner_groups,
        Err(err) => {
            tracing::error!(
                ?err,
                "Can't get existing user_owner: Failed to query database"
            );

            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::empty())
                .unwrap();
        }
    };

    let html = render_to_string(|| {
        view! {
            <Base is_logged_in={true}>
                <h1 class="text-2xl font-bold mb-4">Your Owner Groups</h1>
                <div hx-boost="true" class="flex flex-col gap-4">
                    {owner_groups.into_iter().map(|record|{ view!{ 
                        <div class="bg-neutral text-info py-4 px-8 cursor-pointer w-full rounded-lg transition-all outline outline-transparent hover:outline-blue-500">
                            {
                                let id = record.id.to_string();
                                let name = record.name;
                                view!{
                                    <a href={format!("/owner/{}", id.clone())} class="text-sm">
                                        <h2 class="text-2xl font-bold text-white">{name}</h2>
                                        <span class="text-sm text-neutral-content">{"Group ID: "}{id}</span>
                                    </a>
                                }
                            }
                        </div>
                    }}).collect::<Vec<_>>()}
                </div>
            </Base>
        }
    })
    .into_owned();

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/html")
        .body(Body::from(html))
        .unwrap()
}

pub async fn router(_state: AppState, _config: &Settings) -> Router<AppState, Body> {
    Router::new()
        .route_with_tsr("/user/suggestions", post(project_owner_suggestions))
        .route_with_tsr(
            "/owner",
            get(project_owner_group_list_ui).post(create_project_owner),
        )
        .route_with_tsr(
            "/owner/:owner_id",
            get(project_owner_group_details_ui).post(update_project_owner),
        )
        .route_with_tsr(
            "/owner/:owner_id/invite",
            get(project_owner_invite_member_ui).post(invite_project_member),
        )
        .route_layer(middleware::from_fn(auth))
}
