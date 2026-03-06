use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router, Server,
};
use chat_serve::server::database::Database;
use serde::Serialize;
use std::sync::Arc;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tower_http::cors::CorsLayer;
use tracing::{error, info};
use uuid::Uuid;

/// Estado compartilhado do servidor
#[derive(Clone)]
struct AppState {
    db: Arc<Database>,
    upload_dir: String,
}

/// Resposta de upload bem-sucedido
#[derive(Serialize)]
struct UploadResponse {
    file_id: String,
    filename: String,
    file_size: u64,
    mime_type: String,
}

/// Erro customizado
#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Inicializa logging
    let env_filter = tracing_subscriber::EnvFilter::from_default_env();
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .init();

    // Configurações
    let db_path = std::env::var("DB_PATH").unwrap_or_else(|_| "users.db".to_string());
    let upload_dir = std::env::var("UPLOAD_DIR").unwrap_or_else(|_| "uploads".to_string());
    let port = std::env::var("FILE_SERVER_PORT")
        .unwrap_or_else(|_| "3001".to_string())
        .parse::<u16>()?;

    // Criar diretório de uploads
    fs::create_dir_all(&upload_dir).await?;

    // Inicializar database
    let db = Arc::new(Database::new(&db_path)?);

    // Estado compartilhado
    let state = AppState {
        db,
        upload_dir: upload_dir.clone(),
    };

    // Rotas
    let app = Router::new()
        .route("/upload", post(upload_handler))
        .route("/upload/:file_id", post(upload_file_handler))
        .route("/download/:file_id/:token", get(download_handler))
        .route("/health", get(health_check))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = format!("0.0.0.0:{}", port);
    info!("📁 File Server rodando em http://{}", addr);
    info!("📂 Diretório de uploads: {}", upload_dir);

    Server::bind(&addr.parse()?)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}

/// Health check endpoint
async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "service": "file_server"
    }))
}

/// Handler de upload (multipart/form-data)
/// Recebe JSON com filename, file_size, uploaded_by
/// Arquivo é enviado em seguida
async fn upload_handler(
    State(state): State<AppState>,
    Json(metadata): Json<UploadMetadata>,
) -> Result<Json<UploadResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Gerar ID único para o arquivo
    let file_id = Uuid::new_v4().to_string();

    // Detectar MIME type
    let mime_type = mime_guess::from_path(&metadata.filename)
        .first_or_octet_stream()
        .to_string();

    // Path onde salvar o arquivo
    let file_path = format!("{}/{}", state.upload_dir, file_id);

    // Criar entrada no banco
    state
        .db
        .save_file_metadata(
            &file_id,
            &metadata.filename,
            metadata.file_size,
            &mime_type,
            &metadata.uploaded_by,
            &file_path,
        )
        .map_err(|e| {
            error!("Erro ao salvar metadados: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Erro ao salvar metadados".to_string(),
                }),
            )
        })?;

    info!(
        file_id=%file_id,
        filename=%metadata.filename,
        size=%metadata.file_size,
        uploaded_by=%metadata.uploaded_by,
        "✅ Metadados salvos - aguardando arquivo"
    );

    Ok(Json(UploadResponse {
        file_id,
        filename: metadata.filename,
        file_size: metadata.file_size,
        mime_type,
    }))
}

/// Metadados de upload
#[derive(serde::Deserialize)]
struct UploadMetadata {
    filename: String,
    file_size: u64,
    uploaded_by: String,
}

/// Handler para receber os bytes do arquivo
async fn upload_file_handler(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
    body: Bytes,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // Verificar se file_id existe
    let file_info = state.db.get_file_info(&file_id).map_err(|e| {
        error!("Arquivo não encontrado: {}", e);
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Arquivo não encontrado".to_string(),
            }),
        )
    })?;

    // Salvar arquivo no disco
    let mut file = fs::File::create(&file_info.file_path).await.map_err(|e| {
        error!("Erro ao criar arquivo: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Erro ao salvar arquivo".to_string(),
            }),
        )
    })?;

    file.write_all(&body).await.map_err(|e| {
        error!("Erro ao escrever arquivo: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Erro ao salvar arquivo".to_string(),
            }),
        )
    })?;

    info!(
        file_id=%file_id,
        size=%body.len(),
        "✅ Upload concluído"
    );

    Ok(StatusCode::CREATED)
}

/// Handler de download com token
async fn download_handler(
    State(state): State<AppState>,
    Path((file_id, token)): Path<(String, String)>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    // Validar token
    let valid = state
        .db
        .validate_download_token(&file_id, &token)
        .map_err(|e| {
            error!("Erro ao validar token: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Erro ao validar token".to_string(),
                }),
            )
        })?;

    if !valid {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "Token inválido ou expirado".to_string(),
            }),
        ));
    }

    // Obter informações do arquivo
    let file_info = state.db.get_file_info(&file_id).map_err(|e| {
        error!("Erro ao buscar arquivo: {}", e);
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Arquivo não encontrado".to_string(),
            }),
        )
    })?;

    // Ler arquivo do disco
    let file_data = fs::read(&file_info.file_path).await.map_err(|e| {
        error!("Erro ao ler arquivo do disco: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Erro ao ler arquivo".to_string(),
            }),
        )
    })?;

    info!(
        file_id=%file_id,
        filename=%file_info.filename,
        "📥 Download realizado"
    );

    // Retornar arquivo com headers apropriados
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, file_info.mime_type),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{}\"", file_info.filename),
            ),
            (header::CONTENT_LENGTH, file_info.file_size.to_string()),
        ],
        file_data,
    )
        .into_response())
}
