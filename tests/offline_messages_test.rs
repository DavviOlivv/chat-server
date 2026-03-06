use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};

use chat_serve::model::message::{AckKind, ChatMessage};
use chat_serve::server::auth::AuthManager;
use chat_serve::server::core::ChatCore;
use chat_serve::server::database::Database;
use chat_serve::server::state::ChatState;

/// Helper: inicia servidor com banco de dados para teste de offline messages
async fn start_test_server_with_db() -> (tokio::task::JoinHandle<()>, String, String) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let bind_addr = format!("127.0.0.1:{}", addr.port());

    // Usa banco em memória para teste
    let db_path = ":memory:";
    let db = Arc::new(Database::new(db_path).expect("Falha ao criar DB"));

    let auth_path = format!("test_offline_users_{}.json", addr.port());
    std::env::set_var("DB_PATH", db_path);
    let auth = Arc::new(AuthManager::new(&auth_path));

    let state = Arc::new(ChatState::new());
    state.init_fixed_rooms();
    let core = Arc::new(ChatCore::new_with_auth_and_db(state.clone(), auth, db));

    let handle = tokio::spawn(async move {
        while let Ok((socket, addr)) = listener.accept().await {
            let core_handler = core.clone();
            tokio::spawn(async move {
                chat_serve::client::handle::handle_connection(socket, addr, core_handler).await;
            });
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;
    (handle, bind_addr, auth_path)
}

async fn connect_client(
    addr: &str,
) -> (
    BufReader<tokio::net::tcp::OwnedReadHalf>,
    tokio::net::tcp::OwnedWriteHalf,
) {
    let stream = TcpStream::connect(addr).await.unwrap();
    let (reader, writer) = stream.into_split();
    (BufReader::new(reader), writer)
}

async fn send_msg(writer: &mut tokio::net::tcp::OwnedWriteHalf, msg: &ChatMessage) {
    let json = serde_json::to_string(msg).unwrap();
    writer
        .write_all(format!("{}\n", json).as_bytes())
        .await
        .unwrap();
}

async fn read_msg(reader: &mut BufReader<tokio::net::tcp::OwnedReadHalf>) -> Option<ChatMessage> {
    for _ in 0..5 {
        let mut line = String::new();
        match timeout(Duration::from_secs(2), reader.read_line(&mut line)).await {
            Ok(Ok(n)) if n > 0 => {
                if let Ok(msg) = serde_json::from_str::<ChatMessage>(&line) {
                    match msg {
                        ChatMessage::Ack {
                            kind: AckKind::System,
                            ..
                        } => continue,
                        _ => return Some(msg),
                    }
                }
            }
            _ => return None,
        }
    }
    None
}

#[tokio::test]
async fn test_offline_message_delivery() {
    let (_server, addr, _auth_path) = start_test_server_with_db().await;

    // 1. Registrar Alice e Bob
    let (mut reader_alice, mut writer_alice) = connect_client(&addr).await;
    send_msg(
        &mut writer_alice,
        &ChatMessage::Register {
            username: "alice".to_string(),
            password: "pass123".to_string(),
        },
    )
    .await;

    // Aguardar token de Alice
    let alice_token = loop {
        if let Some(ChatMessage::SessionToken { token, .. }) = read_msg(&mut reader_alice).await {
            break token;
        }
    };

    // Registrar Bob mas não conectar ainda
    let (mut reader_bob_reg, mut writer_bob_reg) = connect_client(&addr).await;
    send_msg(
        &mut writer_bob_reg,
        &ChatMessage::Register {
            username: "bob".to_string(),
            password: "pass456".to_string(),
        },
    )
    .await;

    // Aguardar confirmação de Bob e desconectar
    if read_msg(&mut reader_bob_reg).await.is_some() {
        // Confirmação recebida
    }
    drop(reader_bob_reg);
    drop(writer_bob_reg);

    tokio::time::sleep(Duration::from_millis(500)).await;

    // 2. Alice envia DM para Bob (que está offline agora)
    let dm1 = ChatMessage::Authenticated {
        token: alice_token.clone(),
        action: Box::new(chat_serve::model::message::ChatAction::Private {
            from: "alice".to_string(),
            to: "bob".to_string(),
            content: "Mensagem offline 1".to_string(),
            timestamp: chrono::Utc::now(),
            message_id: Some("msg-001".to_string()),
        }),
    };
    send_msg(&mut writer_alice, &dm1).await;

    // Consumir resposta de Alice (pode ser ACK ou outro)
    let _ = read_msg(&mut reader_alice).await;

    // Alice envia segunda mensagem
    let dm2 = ChatMessage::Authenticated {
        token: alice_token.clone(),
        action: Box::new(chat_serve::model::message::ChatAction::Private {
            from: "alice".to_string(),
            to: "bob".to_string(),
            content: "Mensagem offline 2".to_string(),
            timestamp: chrono::Utc::now(),
            message_id: Some("msg-002".to_string()),
        }),
    };
    send_msg(&mut writer_alice, &dm2).await;
    let _ = read_msg(&mut reader_alice).await; // Consome ACK

    tokio::time::sleep(Duration::from_millis(500)).await;

    // 3. Bob faz login - deve receber as 2 mensagens pendentes
    let (mut reader_bob, mut writer_bob) = connect_client(&addr).await;
    send_msg(
        &mut writer_bob,
        &ChatMessage::Login {
            username: "bob".to_string(),
            password: "pass456".to_string(),
        },
    )
    .await;

    // Aguardar token
    let _bob_token = loop {
        if let Some(ChatMessage::SessionToken { token, .. }) = read_msg(&mut reader_bob).await {
            break token;
        }
    };

    // Bob deve receber as 2 mensagens offline
    let mut received_messages = Vec::new();
    for _ in 0..5 {
        if let Some(ChatMessage::Private { content, .. }) = read_msg(&mut reader_bob).await {
            received_messages.push(content);
        }
        if received_messages.len() >= 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    assert_eq!(
        received_messages.len(),
        2,
        "Bob deveria receber 2 mensagens offline"
    );
    assert!(received_messages.contains(&"Mensagem offline 1".to_string()));
    assert!(received_messages.contains(&"Mensagem offline 2".to_string()));
}

#[tokio::test]
async fn test_offline_message_not_for_nonexistent_user() {
    let (_server, addr, _auth_path) = start_test_server_with_db().await;

    // Registrar e fazer login como Alice
    let (mut reader, mut writer) = connect_client(&addr).await;
    send_msg(
        &mut writer,
        &ChatMessage::Register {
            username: "alice".to_string(),
            password: "pass123".to_string(),
        },
    )
    .await;

    let token = loop {
        if let Some(ChatMessage::SessionToken { token, .. }) = read_msg(&mut reader).await {
            break token;
        }
    };

    // Tentar enviar DM para usuário inexistente
    let dm = ChatMessage::Authenticated {
        token,
        action: Box::new(chat_serve::model::message::ChatAction::Private {
            from: "alice".to_string(),
            to: "ghost".to_string(),
            content: "Teste".to_string(),
            timestamp: chrono::Utc::now(),
            message_id: Some("msg-ghost".to_string()),
        }),
    };
    send_msg(&mut writer, &dm).await;

    // Deve receber erro, não mensagem offline
    let response = read_msg(&mut reader).await;
    assert!(response.is_some());
    match response.unwrap() {
        ChatMessage::Error(msg) => {
            assert!(msg.contains("ghost") || msg.contains("não encontrado"));
        }
        _ => panic!("Deveria receber erro para usuário inexistente"),
    }
}

#[tokio::test]
async fn test_online_message_not_saved_as_offline() {
    let (_server, addr, _auth_path) = start_test_server_with_db().await;

    // Registrar Alice
    let (mut reader_alice, mut writer_alice) = connect_client(&addr).await;
    send_msg(
        &mut writer_alice,
        &ChatMessage::Register {
            username: "alice".to_string(),
            password: "pass123".to_string(),
        },
    )
    .await;

    let alice_token = loop {
        if let Some(ChatMessage::SessionToken { token, .. }) = read_msg(&mut reader_alice).await {
            break token;
        }
    };

    // Registrar Bob e manter conectado
    let (mut reader_bob, mut writer_bob) = connect_client(&addr).await;
    send_msg(
        &mut writer_bob,
        &ChatMessage::Register {
            username: "bob".to_string(),
            password: "pass456".to_string(),
        },
    )
    .await;

    let _bob_token = loop {
        if let Some(ChatMessage::SessionToken { token, .. }) = read_msg(&mut reader_bob).await {
            break token;
        }
    };

    tokio::time::sleep(Duration::from_millis(300)).await;

    // Alice envia DM para Bob (ONLINE)
    let dm = ChatMessage::Authenticated {
        token: alice_token,
        action: Box::new(chat_serve::model::message::ChatAction::Private {
            from: "alice".to_string(),
            to: "bob".to_string(),
            content: "Oi Bob!".to_string(),
            timestamp: chrono::Utc::now(),
            message_id: Some("msg-online".to_string()),
        }),
    };
    send_msg(&mut writer_alice, &dm).await;

    // Alice deve receber ACK de "entregue" (não "offline")
    let ack = read_msg(&mut reader_alice).await;
    match ack.unwrap() {
        ChatMessage::Ack { kind, info, .. } => {
            assert_eq!(kind, AckKind::Delivered);
            assert!(info.contains("entregue"));
            assert!(!info.contains("offline"));
        }
        _ => panic!("Esperado ACK de entrega"),
    }

    // Bob deve receber a mensagem imediatamente
    let msg = read_msg(&mut reader_bob).await;
    match msg.unwrap() {
        ChatMessage::Private { content, .. } => {
            assert_eq!(content, "Oi Bob!");
        }
        _ => panic!("Bob deveria receber mensagem privada"),
    }
}
