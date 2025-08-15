use axum::extract::{State, Path};
use axum::response::Response;
use axum::Json;
use hyper::{Body, StatusCode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::{auth::Auth, startup::AppState};

#[derive(Deserialize, Debug)]
pub struct BulkUpdateProjectEnvironRequest {
    pub envs: HashMap<String, String>,
}

#[derive(Serialize, Debug)]
struct ErrorResponse {
    message: String
}

#[tracing::instrument(skip(auth, pool))]
pub async fn post(
    auth: Auth,
    State(AppState { pool, domain, secure, .. }): State<AppState>,
    Path((owner, project)): Path<(String, String)>,
    Json(req): Json<BulkUpdateProjectEnvironRequest>
) -> Response<Body> {
    let _user = auth.current_user.unwrap();

    let BulkUpdateProjectEnvironRequest { envs } = req;

    // check if project exist
    let project = match sqlx::query_as::<_, (uuid::Uuid, String, serde_json::Value)>(
        r#"SELECT projects.id AS id, projects.name AS project, projects.environs AS env
           FROM projects
           JOIN project_owners ON projects.owner_id = project_owners.id
           JOIN users_owners ON project_owners.id = users_owners.owner_id
           WHERE projects.name = $1
           AND project_owners.name = $2
        "#
    )
    .bind(&project)
    .bind(&owner)
    .fetch_optional(&pool)
    .await
    {
        Ok(Some(record)) => record,
        Ok(None) => {
            let json = serde_json::to_string(&ErrorResponse {
                message: "Project does not exist".to_string()
            }).unwrap();

            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from(json))
                .unwrap();
        }
        Err(err) => {
            tracing::error!(?err, "Can't get projects: Failed to query database");

            let json = serde_json::to_string(&ErrorResponse {
                message: format!("Failed to query database: {}", err.to_string())
            }).unwrap();

            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from(json))
                .unwrap();
        }
    };

    let project_id = project.0;

    // Convert HashMap to JSON value for bulk update
    let envs_json = match serde_json::to_value(&envs) {
        Ok(json) => json,
        Err(err) => {
            let json = serde_json::to_string(&ErrorResponse {
                message: format!("Failed to serialize environment variables: {}", err.to_string())
            }).unwrap();

            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from(json))
                .unwrap();
        }
    };

    // Bulk replace all environment variables
    match sqlx::query(
        r#"UPDATE projects
            SET environs = $1
            WHERE id = $2
        "#
    )
    .bind(&envs_json)
    .bind(&project_id)
    .execute(&pool)
    .await {
        Ok(data) => data,
        Err(err) => {
            tracing::error!(
                ?err,
                "Can't bulk update project environs: Failed to update database"
            );

            let json = serde_json::to_string(&ErrorResponse {
                message: "Failed to update database".to_string()
            }).unwrap();

            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from(json))
                .unwrap();
        }    
    };

    Response::builder()
        .status(StatusCode::NO_CONTENT)
        .body(Body::empty())
        .unwrap()
}
