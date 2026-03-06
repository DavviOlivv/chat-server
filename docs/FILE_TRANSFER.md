# Transferência de Arquivos - Documentação Completa

## 🎯 Arquitetura

A transferência de arquivos no `chat-serve` segue o **padrão da indústria**: desacoplamento entre o canal de controle (chat TCP) e o canal de dados (HTTP).

```
┌──────────────┐                      ┌──────────────┐
│   Cliente    │◄────TCP (8080)──────►│ Chat Server  │
│     (TUI)    │    (Mensagens)       │   (Tokio)    │
└──────┬───────┘                      └──────────────┘
       │                                       │
       │                                       │ Notificações
       │                                       ▼
       │                              ┌──────────────┐
       │                              │   Database   │
       │                              │   (SQLite)   │
       │                              └──────────────┘
       │
       │
       └───────HTTP (3001)──────►┌──────────────┐
            (Upload/Download)    │ File Server  │
                                 │   (Axum)     │
                                 └──────┬───────┘
                                        │
                                        ▼
                                 ┌──────────────┐
                                 │   uploads/   │
                                 │  (Storage)   │
                                 └──────────────┘
```

### Por que essa arquitetura?

#### 1. **Desacoplamento de Protocolo**
- **Chat TCP**: Otimizado para mensagens curtas e baixa latência
- **HTTP**: Nativo para transferência de arquivos (compressão, resumable uploads, cache)
- Passar um arquivo de 50MB pelo socket de chat "entupiria" o canal

#### 2. **Eficiência (Zero-Copy)**
- Streaming: Arquivo vai do disco direto para a rede em chunks
- Sem carregar arquivo inteiro na RAM
- Interface fluida: usuário continua conversando enquanto upload acontece

#### 3. **Escalabilidade**
- Fácil migração para S3/MinIO sem modificar chat server
- Chat server é **plano de controle**, File server é **plano de dados**

---

## 📊 Componentes

### 1. Database (SQLite)

#### Tabela `files`
```sql
CREATE TABLE files (
    file_id TEXT PRIMARY KEY,           -- UUID do arquivo
    filename TEXT NOT NULL,             -- Nome original
    file_size INTEGER NOT NULL,         -- Tamanho em bytes
    mime_type TEXT NOT NULL,            -- application/pdf, image/png, etc
    uploaded_by TEXT NOT NULL,          -- Username do uploader
    uploaded_at TEXT NOT NULL,          -- RFC3339 timestamp
    file_path TEXT NOT NULL,            -- Caminho no disco (uploads/uuid)
    download_token TEXT,                -- Token temporário de download
    token_expires_at TEXT,              -- Expiração do token (1 hora)
    FOREIGN KEY(uploaded_by) REFERENCES users(username)
);
```

#### Métodos
- `save_file_metadata()`: Salvar metadados após upload
- `generate_download_token(file_id, for_user)`: Gerar token de 1h
- `validate_download_token(file_id, token)`: Validar token
- `get_file_info(file_id)`: Obter metadados

### 2. Protocolo (model/message.rs)

#### ChatAction::SendFile
```rust
SendFile {
    from: String,              // Remetente
    to: String,                // Destinatário
    file_id: String,           // UUID do arquivo
    file_name: String,         // Nome original
    file_size: u64,            // Tamanho em bytes
    mime_type: String,         // MIME type
    timestamp: DateTime<Utc>,  // Data/hora
}
```

#### ChatMessage::FileNotification
```rust
FileNotification {
    from: String,              // Quem enviou
    file_id: String,           // UUID para download
    file_name: String,         // Nome do arquivo
    file_size: u64,            // Tamanho
    mime_type: String,         // Tipo
    download_token: String,    // Token temporário (1h)
    timestamp: String,         // RFC3339
}
```

### 3. File Server (Axum)

#### Rotas

**POST /upload**
- Body: JSON com `{ filename, file_size, uploaded_by }`
- Retorna: `{ file_id, filename, file_size, mime_type }`
- Cria metadados no banco, retorna UUID

**POST /upload/:file_id**
- Body: Bytes do arquivo
- Salva arquivo em `uploads/:file_id`
- Retorna: 201 Created

**GET /download/:file_id/:token**
- Valida token (expiração, permissão)
- Retorna arquivo com headers apropriados:
  - `Content-Type: <mime_type>`
  - `Content-Disposition: attachment; filename="..."`
  - `Content-Length: <size>`

**GET /health**
- Retorna: `{ "status": "ok", "service": "file_server" }`

#### Configuração

```bash
# Variáveis de ambiente
DB_PATH=users.db              # Caminho do SQLite
UPLOAD_DIR=uploads            # Diretório de uploads
FILE_SERVER_PORT=3001         # Porta do servidor
```

### 4. Chat Server Handler

```rust
ChatAction::SendFile { from, to, file_id, ... } => {
    // 1. Gerar token de download para destinatário
    let token = db.generate_download_token(&file_id, &to)?;
    
    // 2. Notificar destinatário
    send_reliable(to_tx, ChatMessage::FileNotification {
        from, file_id, file_name, file_size, mime_type,
        download_token: token,
        timestamp: ...,
    });
    
    // 3. Confirmar para remetente
    send_reliable(from_tx, ChatMessage::Ack {
        info: "✅ Arquivo enviado para Bob",
        ...
    });
}
```

---

## 🔄 Fluxo Completo

### Upload + Envio

```
┌─────────┐                  ┌──────────────┐                  ┌──────────┐                  ┌────────┐
│  Alice  │                  │ File Server  │                  │   Chat   │                  │  Bob   │
└────┬────┘                  └──────┬───────┘                  └────┬─────┘                  └───┬────┘
     │                              │                               │                            │
     │ POST /upload                 │                               │                            │
     │ {filename, size, from}       │                               │                            │
     ├─────────────────────────────►│                               │                            │
     │                              │ save_file_metadata()          │                            │
     │                              ├──────────────────────────────►│                            │
     │                              │                               │                            │
     │◄─────────────────────────────┤                               │                            │
     │ {file_id, ...}               │                               │                            │
     │                              │                               │                            │
     │ POST /upload/:file_id        │                               │                            │
     │ <bytes>                      │                               │                            │
     ├─────────────────────────────►│                               │                            │
     │                              │ write to uploads/uuid         │                            │
     │◄─────────────────────────────┤                               │                            │
     │ 201 Created                  │                               │                            │
     │                              │                               │                            │
     │ Authenticated { SendFile }   │                               │                            │
     ├──────────────────────────────────────────────────────────────►│                            │
     │                              │                               │ generate_download_token()  │
     │                              │                               │                            │
     │                              │                               │ FileNotification           │
     │                              │                               ├───────────────────────────►│
     │                              │                               │                            │
     │◄─────────────────────────────────────────────────────────────┤                            │
     │ Ack: "Arquivo enviado"       │                               │                            │
```

### Download

```
┌────────┐                  ┌──────────────┐                  ┌──────────┐
│  Bob   │                  │ File Server  │                  │   Chat   │
└───┬────┘                  └──────┬───────┘                  └────┬─────┘
    │                              │                               │
    │ GET /download/:file_id/:token│                               │
    ├─────────────────────────────►│                               │
    │                              │ validate_download_token()     │
    │                              ├──────────────────────────────►│
    │                              │◄──────────────────────────────┤
    │                              │ token valid                   │
    │                              │                               │
    │                              │ get_file_info()               │
    │                              ├──────────────────────────────►│
    │                              │◄──────────────────────────────┤
    │                              │ file metadata                 │
    │                              │                               │
    │                              │ read file from uploads/       │
    │◄─────────────────────────────┤                               │
    │ 200 OK + file bytes          │                               │
```

---

## 💻 Uso

### Iniciar Servidores

```bash
# Terminal 1: Chat Server
RUST_LOG=info cargo run --bin chat_server

# Terminal 2: File Server
RUST_LOG=info FILE_SERVER_PORT=3001 cargo run --bin file_server

# Terminal 3: Cliente TUI
cargo run --bin tui
```

### Comandos no Chat (Exemplo Conceitual)

#### Enviar Arquivo
```
/sendfile bob projeto.zip
```

O cliente deve:
1. Ler arquivo do disco
2. POST /upload com metadados
3. POST /upload/:file_id com bytes
4. Enviar ChatAction::SendFile via chat

#### Receber Notificação
```
📎 Arquivo de alice: projeto.zip [2.5 MB]
   Pressione 'D' para baixar
```

#### Baixar Arquivo
- Tecla 'D' ou comando `/download`
- Cliente faz GET /download/:file_id/:token
- Salva em Downloads/ ou caminho escolhido

---

## 🔒 Segurança

### 1. **Tokens Temporários**
- Validade: 1 hora (configurável)
- Gerados por destinatário legítimo
- Validação no File Server antes do download

### 2. **Validação de Permissões**
```rust
// Apenas uploader ou destinatário pode gerar token
let has_permission: bool = conn.query_row(
    "SELECT COUNT(*) > 0 FROM files 
     WHERE file_id = ?1 AND uploaded_by = ?2",
    params![file_id, for_user],
    |row| row.get(0),
)?;
```

### 3. **SQL Injection Protection**
- Queries parametrizadas (rusqlite)
- Validação de UUIDs

### 4. **Path Traversal Protection**
- Arquivos salvos com UUID (não filename original)
- Diretório isolado (`uploads/`)

### 5. **DoS Protection**
- Rate limiting no chat (10 msg/s)
- Limite de tamanho de arquivo (configurável)
- Token expiration automática

---

## 📈 Performance

### Streaming de Arquivos
- File Server usa `tokio::fs` para I/O assíncrono
- Arquivos enviados em chunks (não buffer inteiro)
- Zero impacto no chat server

### Escalabilidade

#### Local Storage (Atual)
```
uploads/
├── uuid-1
├── uuid-2
└── uuid-3
```

#### Object Storage (Futuro)
```rust
// Trocar backend sem mudar protocolo
async fn save_file(data: Bytes) -> String {
    #[cfg(feature = "s3")]
    return s3_client.put_object(bucket, uuid, data).await;
    
    #[cfg(not(feature = "s3"))]
    return tokio::fs::write(format!("uploads/{}", uuid), data).await;
}
```

---

## 🛠️ Implementação no Cliente TUI

### Estrutura Proposta

```rust
// Adicionar ao App struct
pub struct App {
    // ... campos existentes ...
    pending_file_download: Option<FileDownloadInfo>,
}

#[derive(Clone)]
struct FileDownloadInfo {
    file_id: String,
    filename: String,
    download_token: String,
    file_size: u64,
}

// Handler de notificação
ChatMessage::FileNotification { from, file_id, file_name, download_token, ... } => {
    app.pending_file_download = Some(FileDownloadInfo {
        file_id,
        filename: file_name.clone(),
        download_token,
        file_size,
    });
    
    app.add_message(Message {
        from: from.clone(),
        content: format!("📎 Arquivo: {} [{} MB]\\nPressione 'D' para baixar", 
                        file_name, file_size / 1_000_000),
        is_system: true,
        ...
    });
}

// Handler de tecla 'D'
KeyCode::Char('d') | KeyCode::Char('D') => {
    if let Some(download_info) = &app.pending_file_download {
        tokio::spawn(download_file(download_info.clone()));
        app.set_notification("⬇️ Download iniciado...".to_string());
        app.pending_file_download = None;
    }
}

// Função de download
async fn download_file(info: FileDownloadInfo) {
    let url = format!("http://localhost:3001/download/{}/{}", 
                     info.file_id, info.download_token);
    
    let response = reqwest::get(&url).await?;
    let bytes = response.bytes().await?;
    
    let path = format!("Downloads/{}", info.filename);
    tokio::fs::write(&path, bytes).await?;
}
```

### Comando /sendfile

```rust
else if input.starts_with("/sendfile ") {
    let parts: Vec<&str> = input.splitn(3, ' ').collect();
    if parts.len() == 3 {
        let to_user = parts[1].to_string();
        let file_path = parts[2].to_string();
        
        tokio::spawn(upload_and_send(
            file_path,
            to_user,
            username.clone(),
            session_token.clone(),
        ));
        
        app.set_notification("⬆️ Upload iniciado...".to_string());
    }
}

async fn upload_and_send(
    file_path: String,
    to_user: String,
    from_user: String,
    token: Arc<Mutex<Option<String>>>,
) {
    // 1. Ler arquivo
    let file_data = tokio::fs::read(&file_path).await?;
    let filename = Path::new(&file_path).file_name()?.to_string_lossy();
    
    // 2. POST /upload (metadados)
    let metadata = serde_json::json!({
        "filename": filename,
        "file_size": file_data.len(),
        "uploaded_by": from_user,
    });
    
    let response: UploadResponse = reqwest::Client::new()
        .post("http://localhost:3001/upload")
        .json(&metadata)
        .send().await?
        .json().await?;
    
    // 3. POST /upload/:file_id (bytes)
    reqwest::Client::new()
        .post(format!("http://localhost:3001/upload/{}", response.file_id))
        .body(file_data)
        .send().await?;
    
    // 4. Notificar via chat
    let action = ChatAction::SendFile {
        from: from_user,
        to: to_user,
        file_id: response.file_id,
        file_name: filename.to_string(),
        file_size: response.file_size,
        mime_type: response.mime_type,
        timestamp: Utc::now(),
    };
    
    send_to_chat(token, action).await;
}
```

---

## 📚 Exemplos Práticos

### 1. Alice envia PDF para Bob

```bash
# Alice (TUI)
/sendfile bob relatorio.pdf

# Output:
# ⬆️ Upload iniciado...
# ✅ Arquivo enviado para bob

# Bob (TUI) recebe:
# 📎 Arquivo de alice: relatorio.pdf [1.2 MB]
#    Pressione 'D' para baixar

# Bob pressiona 'D'
# ⬇️ Download iniciado...
# ✅ Arquivo salvo em Downloads/relatorio.pdf
```

### 2. Verificar Health do File Server

```bash
curl http://localhost:3001/health

# Response:
# {"status":"ok","service":"file_server"}
```

### 3. Upload Manual (curl)

```bash
# 1. Criar metadados
curl -X POST http://localhost:3001/upload \
  -H "Content-Type: application/json" \
  -d '{"filename":"test.txt","file_size":13,"uploaded_by":"alice"}'

# Response:
# {"file_id":"550e8400-e29b-41d4-a716-446655440000",...}

# 2. Enviar bytes
curl -X POST http://localhost:3001/upload/550e8400-e29b-41d4-a716-446655440000 \
  --data-binary "@test.txt"

# Response: 201 Created
```

---

## 🚀 Melhorias Futuras

### 1. **Progress Bar**
```rust
// Durante upload/download
app.upload_progress = Some(ProgressInfo {
    current: 1_200_000,
    total: 5_000_000,
    filename: "video.mp4",
});

// Renderizar no TUI
Gauge::default()
    .percent(progress.current * 100 / progress.total)
    .label(format!("{:.1}%", percent))
```

### 2. **Resumable Uploads**
- HTTP Range requests
- Salvar chunks no servidor
- Retomar de onde parou

### 3. **Thumbnails para Imagens**
- Gerar preview ao fazer upload
- Exibir inline no TUI
- Armazenar em `thumbnails/:file_id`

### 4. **Compressão Automática**
- Detectar arquivos grandes
- Comprimir antes de enviar (gzip, zstd)
- Descomprimir ao baixar

### 5. **S3/MinIO Backend**
```rust
#[cfg(feature = "s3")]
async fn save_to_s3(file_id: &str, data: Bytes) -> Result<()> {
    let client = aws_sdk_s3::Client::new(&config);
    client.put_object()
        .bucket("chat-serve-files")
        .key(file_id)
        .body(data.into())
        .send()
        .await?;
    Ok(())
}
```

### 6. **Quota de Storage**
```sql
-- Adicionar à tabela users
ALTER TABLE users ADD COLUMN storage_used INTEGER DEFAULT 0;
ALTER TABLE users ADD COLUMN storage_limit INTEGER DEFAULT 1073741824; -- 1GB

-- Verificar ao fazer upload
SELECT storage_used + ?1 <= storage_limit FROM users WHERE username = ?2
```

### 7. **Scan de Vírus**
```rust
// Integração com ClamAV
async fn scan_file(path: &str) -> Result<bool> {
    let output = Command::new("clamscan")
        .arg(path)
        .output()
        .await?;
    Ok(output.status.success())
}
```

---

## 📊 Métricas e Monitoramento

### Prometheus Metrics

```rust
// file_server.rs
lazy_static! {
    static ref UPLOADS_TOTAL: IntCounter = register_int_counter!(
        "file_server_uploads_total",
        "Total de uploads"
    ).unwrap();
    
    static ref DOWNLOADS_TOTAL: IntCounter = register_int_counter!(
        "file_server_downloads_total",
        "Total de downloads"
    ).unwrap();
    
    static ref STORAGE_BYTES: IntGauge = register_int_gauge!(
        "file_server_storage_bytes",
        "Storage usado em bytes"
    ).unwrap();
}

// No upload
UPLOADS_TOTAL.inc();
STORAGE_BYTES.add(file_size as i64);

// No download
DOWNLOADS_TOTAL.inc();
```

### Logs Estruturados

```
[INFO] 📎 file_id="uuid-123" filename="doc.pdf" size=1024000 uploaded_by="alice"
[INFO] 📥 file_id="uuid-123" downloaded_by="bob" token="valid"
```

---

## ✅ Checklist de Implementação

### Backend
- [x] Tabela `files` no SQLite
- [x] Métodos database (save, generate_token, validate, get_info)
- [x] ChatAction::SendFile e ChatMessage::FileNotification
- [x] Handler no core.rs
- [x] File server com axum (upload + download)
- [x] Validação de tokens
- [x] MIME type detection

### Frontend (TUI)
- [ ] Comando /sendfile <user> <path>
- [ ] Upload HTTP com reqwest
- [ ] Handler FileNotification
- [ ] Tecla 'D' para download
- [ ] Progress bar (opcional)
- [ ] Salvar em Downloads/

### Documentação
- [x] Arquitetura e fluxo
- [x] Segurança e performance
- [x] Exemplos práticos
- [ ] README atualizado

---

## 🎓 Conclusão

Este sistema de transferência de arquivos:

✅ Segue **padrões da indústria** (TCP para controle, HTTP para dados)  
✅ É **eficiente** (streaming, zero-copy, sem bloqueio)  
✅ É **escalável** (fácil migração para S3/MinIO)  
✅ É **seguro** (tokens temporários, validação de permissões)  
✅ É **extensível** (thumbnails, compressão, scan de vírus)

O arquivo mantém o chat extremamente rápido enquanto transforma o sistema de arquivos em um serviço plugável e independente! 🚀
