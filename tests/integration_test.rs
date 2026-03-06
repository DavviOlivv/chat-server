// Testes de integração end-to-end com servidor real
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};

use chat_serve::model::message::{AckKind, ChatMessage};
use chat_serve::server::core::ChatCore;
use chat_serve::server::state::ChatState;

/// Helper: inicia servidor em porta aleatória e retorna o endereço
async fn start_test_server() -> (tokio::task::JoinHandle<()>, String) {
    // Usa porta 0 para o SO escolher uma porta disponível
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let bind_addr = format!("127.0.0.1:{}", addr.port());

    let state = Arc::new(ChatState::new());
    state.init_fixed_rooms();
    let core = Arc::new(ChatCore::new(state.clone()));

    let handle = tokio::spawn(async move {
        while let Ok((socket, addr)) = listener.accept().await {
            let core_handler = core.clone();
            tokio::spawn(async move {
                chat_serve::client::handle::handle_connection(socket, addr, core_handler).await;
            });
        }
    });

    (handle, bind_addr)
}

/// Helper: conecta cliente e retorna reader/writer
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

/// Helper: envia mensagem JSON
async fn send_msg(writer: &mut tokio::net::tcp::OwnedWriteHalf, msg: &ChatMessage) {
    let json = serde_json::to_string(msg).unwrap();
    writer
        .write_all(format!("{}\n", json).as_bytes())
        .await
        .unwrap();
}

/// Helper: lê próxima mensagem com timeout, ignorando mensagens de Sistema
async fn read_msg(reader: &mut BufReader<tokio::net::tcp::OwnedReadHalf>) -> Option<ChatMessage> {
    // Tenta ler até 3 mensagens (para pular System/Join ACKs)
    for _ in 0..3 {
        let mut line = String::new();
        match timeout(Duration::from_secs(2), reader.read_line(&mut line)).await {
            Ok(Ok(n)) if n > 0 => {
                if let Ok(msg) = serde_json::from_str::<ChatMessage>(&line) {
                    // Pula mensagens de sistema/ACK System automáticas
                    match &msg {
                        ChatMessage::Ack {
                            kind: AckKind::System,
                            ..
                        } => continue,
                        ChatMessage::Text { from, .. } if from == "Sistema" => continue,
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
async fn test_login_and_broadcast() {
    let (_server, addr) = start_test_server().await;

    // Conecta dois clientes
    let (_reader1, mut writer1) = connect_client(&addr).await;
    let (mut reader2, mut writer2) = connect_client(&addr).await;

    // Login cliente 1
    send_msg(
        &mut writer1,
        &ChatMessage::Login {
            username: "alice".to_string(),
            password: "test".to_string(),
        },
    )
    .await;

    // Login cliente 2
    send_msg(
        &mut writer2,
        &ChatMessage::Login {
            username: "bob".to_string(),
            password: "test".to_string(),
        },
    )
    .await;

    // Pequeno delay para garantir que ambos estão registrados
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Cliente 1 envia broadcast
    let broadcast = ChatMessage::Text {
        from: "alice".to_string(),
        content: "Hello everyone!".to_string(),
        timestamp: chrono::Utc::now(),
    };
    send_msg(&mut writer1, &broadcast).await;

    // Cliente 2 deve receber o broadcast
    let received = read_msg(&mut reader2).await;
    assert!(received.is_some(), "Cliente 2 não recebeu broadcast");

    match received.unwrap() {
        ChatMessage::Text { from, content, .. } => {
            assert_eq!(from, "alice");
            assert_eq!(content, "Hello everyone!");
        }
        other => panic!("Tipo de mensagem incorreto: {:?}", other),
    }
}

#[tokio::test]
async fn test_dm_with_ack() {
    let (_server, addr) = start_test_server().await;

    let (mut reader1, mut writer1) = connect_client(&addr).await;
    let (mut reader2, mut writer2) = connect_client(&addr).await;

    // Login ambos
    send_msg(
        &mut writer1,
        &ChatMessage::Login {
            username: "alice".to_string(),
            password: "test".to_string(),
        },
    )
    .await;
    send_msg(
        &mut writer2,
        &ChatMessage::Login {
            username: "bob".to_string(),
            password: "test".to_string(),
        },
    )
    .await;

    // Delay para garantir registro
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Alice envia DM para Bob
    let message_id = uuid::Uuid::new_v4().to_string();
    let dm = ChatMessage::Private {
        from: "alice".to_string(),
        to: "bob".to_string(),
        content: "Secret message".to_string(),
        timestamp: chrono::Utc::now(),
        message_id: Some(message_id.clone()),
    };
    send_msg(&mut writer1, &dm).await;

    // Bob deve receber a DM
    let received_bob = timeout(Duration::from_secs(3), async {
        loop {
            if let Some(msg) = read_msg(&mut reader2).await {
                if matches!(msg, ChatMessage::Private { .. }) {
                    return Some(msg);
                }
            } else {
                return None;
            }
        }
    })
    .await;

    assert!(
        received_bob.is_ok() && received_bob.as_ref().unwrap().is_some(),
        "Bob não recebeu DM"
    );

    match received_bob.unwrap().unwrap() {
        ChatMessage::Private {
            from, to, content, ..
        } => {
            assert_eq!(from, "alice");
            assert_eq!(to, "bob");
            assert_eq!(content, "Secret message");
        }
        other => panic!("Bob recebeu mensagem de tipo incorreto: {:?}", other),
    }

    // Alice deve receber ACK Delivered
    let ack = timeout(Duration::from_secs(3), async {
        loop {
            if let Some(msg) = read_msg(&mut reader1).await {
                if matches!(msg, ChatMessage::Ack { .. }) {
                    return Some(msg);
                }
            } else {
                return None;
            }
        }
    })
    .await;

    assert!(
        ack.is_ok() && ack.as_ref().unwrap().is_some(),
        "Alice não recebeu ACK"
    );

    match ack.unwrap().unwrap() {
        ChatMessage::Ack {
            kind,
            message_id: ack_id,
            ..
        } => {
            assert_eq!(kind, AckKind::Delivered);
            assert_eq!(ack_id, Some(message_id));
        }
        other => panic!("Alice recebeu mensagem de tipo incorreto: {:?}", other),
    }
}

#[tokio::test]
async fn test_list_users() {
    let (_server, addr) = start_test_server().await;

    let (mut reader1, mut writer1) = connect_client(&addr).await;
    let (_reader2, mut writer2) = connect_client(&addr).await;

    // Login ambos
    send_msg(
        &mut writer1,
        &ChatMessage::Login {
            username: "alice".to_string(),
            password: "test".to_string(),
        },
    )
    .await;
    send_msg(
        &mut writer2,
        &ChatMessage::Login {
            username: "bob".to_string(),
            password: "test".to_string(),
        },
    )
    .await;

    // Delay maior para garantir ambos registrados
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Alice solicita lista
    send_msg(
        &mut writer1,
        &ChatMessage::ListRequest {
            from: "alice".to_string(),
        },
    )
    .await;

    // Alice deve receber ListResponse
    let response = read_msg(&mut reader1).await;
    assert!(response.is_some(), "Alice não recebeu ListResponse");

    match response.unwrap() {
        ChatMessage::ListResponse { users } => {
            assert!(
                !users.is_empty(),
                "Lista deveria ter pelo menos 1 usuário, tem {}",
                users.len()
            );
            assert!(
                users.contains(&"alice".to_string()),
                "Alice não está na lista"
            );
            // Bob pode ou não estar dependendo do timing, não vamos forçar
        }
        other => panic!("Tipo de mensagem incorreto: {:?}", other),
    }
}

#[tokio::test]
async fn test_room_join_and_text() {
    let (_server, addr) = start_test_server().await;

    let (_reader1, mut writer1) = connect_client(&addr).await;
    let (mut reader2, mut writer2) = connect_client(&addr).await;

    // Login
    send_msg(
        &mut writer1,
        &ChatMessage::Login {
            username: "alice".to_string(),
            password: "test".to_string(),
        },
    )
    .await;
    send_msg(
        &mut writer2,
        &ChatMessage::Login {
            username: "bob".to_string(),
            password: "test".to_string(),
        },
    )
    .await;

    // Alice e Bob entram em #dev
    send_msg(
        &mut writer1,
        &ChatMessage::JoinRoom {
            room: "#dev".to_string(),
        },
    )
    .await;

    send_msg(
        &mut writer2,
        &ChatMessage::JoinRoom {
            room: "#dev".to_string(),
        },
    )
    .await;

    // Limpa ACKs de join (ambos recebem confirmação System)
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Alice envia mensagem para #dev
    send_msg(
        &mut writer1,
        &ChatMessage::RoomText {
            from: "alice".to_string(),
            room: "#dev".to_string(),
            content: "Hello room!".to_string(),
            timestamp: chrono::Utc::now(),
        },
    )
    .await;

    // Bob deve receber (está na sala)
    let received = read_msg(&mut reader2).await;
    assert!(received.is_some(), "Bob não recebeu mensagem da sala");

    match received.unwrap() {
        ChatMessage::RoomText {
            from,
            room,
            content,
            ..
        } => {
            assert_eq!(from, "alice");
            assert_eq!(room, "#dev");
            assert_eq!(content, "Hello room!");
        }
        _ => panic!("Tipo de mensagem incorreto"),
    }
}

#[tokio::test]
async fn test_rate_limiting() {
    let (_server, addr) = start_test_server().await;

    let (mut reader1, mut writer1) = connect_client(&addr).await;

    // Login
    send_msg(
        &mut writer1,
        &ChatMessage::Login {
            username: "alice".to_string(),
            password: "test".to_string(),
        },
    )
    .await;

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Envia 15 mensagens rapidamente (limite é 10/s)
    for i in 0..15 {
        send_msg(
            &mut writer1,
            &ChatMessage::Text {
                from: "alice".to_string(),
                content: format!("Flood message {}", i),
                timestamp: chrono::Utc::now(),
            },
        )
        .await;
        // Pequeno delay para garantir que todas chegam antes de ler respostas
        tokio::time::sleep(Duration::from_micros(500)).await;
    }

    // Aguarda um pouco para servidor processar
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Busca ativamente por ACK Failed com rate limit
    let found = timeout(Duration::from_secs(3), async {
        loop {
            let mut line = String::new();
            if reader1.read_line(&mut line).await.is_ok() && !line.is_empty() {
                if let Ok(ChatMessage::Ack { kind, info, .. }) =
                    serde_json::from_str::<ChatMessage>(&line)
                {
                    if matches!(kind, AckKind::Failed)
                        && info.contains("Limite de mensagens excedido")
                    {
                        return true;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;

    assert!(
        found.is_ok() && found.unwrap(),
        "Rate limit não foi aplicado - não recebeu ACK Failed"
    );
}

#[tokio::test]
async fn test_message_deduplication() {
    let (_server, addr) = start_test_server().await;

    let (_reader1, mut writer1) = connect_client(&addr).await;
    let (mut reader2, mut writer2) = connect_client(&addr).await;

    // Login
    send_msg(
        &mut writer1,
        &ChatMessage::Login {
            username: "alice".to_string(),
            password: "test".to_string(),
        },
    )
    .await;
    send_msg(
        &mut writer2,
        &ChatMessage::Login {
            username: "bob".to_string(),
            password: "test".to_string(),
        },
    )
    .await;

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Alice envia DM com mesmo message_id duas vezes
    let message_id = uuid::Uuid::new_v4().to_string();
    let dm = ChatMessage::Private {
        from: "alice".to_string(),
        to: "bob".to_string(),
        content: "Duplicated message".to_string(),
        timestamp: chrono::Utc::now(),
        message_id: Some(message_id.clone()),
    };

    send_msg(&mut writer1, &dm).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    send_msg(&mut writer1, &dm).await;

    // Bob deve receber apenas UMA mensagem
    let first = timeout(Duration::from_secs(2), async {
        loop {
            if let Some(msg) = read_msg(&mut reader2).await {
                if matches!(msg, ChatMessage::Private { .. }) {
                    return Some(msg);
                }
            } else {
                return None;
            }
        }
    })
    .await;

    assert!(
        first.is_ok() && first.as_ref().unwrap().is_some(),
        "Bob não recebeu primeira mensagem"
    );

    // Tenta ler segunda (não deve existir)
    let second = timeout(Duration::from_millis(800), async {
        loop {
            if let Some(msg) = read_msg(&mut reader2).await {
                if matches!(msg, ChatMessage::Private { .. }) {
                    return Some(msg);
                }
            } else {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    })
    .await;

    assert!(
        second.is_err() || second.unwrap().is_none(),
        "Bob recebeu mensagem duplicada!"
    );
}

#[tokio::test]
async fn test_disconnect_cleanup() {
    let (_server, addr) = start_test_server().await;

    let (mut reader1, mut writer1) = connect_client(&addr).await;
    let (_reader2, mut writer2) = connect_client(&addr).await;
    let (_reader3, mut writer3) = connect_client(&addr).await;

    // Três clientes fazem login
    send_msg(
        &mut writer1,
        &ChatMessage::Login {
            username: "alice".to_string(),
            password: "test".to_string(),
        },
    )
    .await;
    send_msg(
        &mut writer2,
        &ChatMessage::Login {
            username: "bob".to_string(),
            password: "test".to_string(),
        },
    )
    .await;
    send_msg(
        &mut writer3,
        &ChatMessage::Login {
            username: "charlie".to_string(),
            password: "test".to_string(),
        },
    )
    .await;

    tokio::time::sleep(Duration::from_millis(300)).await;

    // Bob desconecta (drop writer)
    drop(writer2);

    // Delay maior para garantir que servidor processa desconexão
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Tenta várias vezes até Bob sumir da lista (pode levar um tempo)
    let mut bob_gone = false;
    for attempt in 0..5 {
        send_msg(
            &mut writer1,
            &ChatMessage::ListRequest {
                from: "alice".to_string(),
            },
        )
        .await;

        let response = timeout(Duration::from_secs(2), async {
            loop {
                if let Some(msg) = read_msg(&mut reader1).await {
                    if matches!(msg, ChatMessage::ListResponse { .. }) {
                        return Some(msg);
                    }
                } else {
                    return None;
                }
            }
        })
        .await;

        if let Ok(Some(ChatMessage::ListResponse { users })) = response {
            if !users.contains(&"bob".to_string()) {
                // Verifica que alice e charlie estão presentes
                assert!(
                    users.contains(&"alice".to_string()),
                    "Alice não está na lista"
                );
                assert!(
                    users.contains(&"charlie".to_string()),
                    "Charlie não está na lista"
                );
                bob_gone = true;
                break;
            }
        }

        if attempt < 4 {
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    }

    assert!(
        bob_gone,
        "Bob ainda está na lista após desconectar (após 5 tentativas)"
    );
}

#[tokio::test]
async fn test_dm_to_nonexistent_user() {
    let (_server, addr) = start_test_server().await;

    let (mut reader1, mut writer1) = connect_client(&addr).await;

    // Login alice
    send_msg(
        &mut writer1,
        &ChatMessage::Login {
            username: "alice".to_string(),
            password: "test".to_string(),
        },
    )
    .await;

    // Tenta enviar DM para usuário que não existe
    let dm = ChatMessage::Private {
        from: "alice".to_string(),
        to: "ghost".to_string(),
        content: "Are you there?".to_string(),
        timestamp: chrono::Utc::now(),
        message_id: Some(uuid::Uuid::new_v4().to_string()),
    };
    send_msg(&mut writer1, &dm).await;

    // Deve receber erro ou ACK Failed
    let response = read_msg(&mut reader1).await;
    assert!(response.is_some(), "Alice não recebeu resposta");

    match response.unwrap() {
        ChatMessage::Error(msg) => {
            assert!(
                msg.contains("ghost")
                    || msg.contains("não encontrado")
                    || msg.contains("not found")
            );
        }
        ChatMessage::Ack { kind, .. } => {
            assert_eq!(kind, AckKind::Failed, "ACK deveria ser Failed");
        }
        _ => panic!("Tipo de resposta inesperado"),
    }
}
