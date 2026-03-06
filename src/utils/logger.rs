use std::net::SocketAddr;

/// Controla se os logs devem expor endereços (por padrão: não)
pub fn show_addrs() -> bool {
    match std::env::var("SHOW_ADDRS") {
        Ok(v) => matches!(v.as_str(), "1" | "true" | "yes"),
        Err(_) => false,
    }
}

/// Formata a identificação do peer para logs: prefere username quando presente,
/// caso contrário exibe o `SocketAddr` apenas se `SHOW_ADDRS` estiver ativado,
/// senão retorna um rótulo neutro `cliente`.
pub fn peer_display(addr: &SocketAddr, username: Option<&str>) -> String {
    if let Some(name) = username {
        name.to_string()
    } else if show_addrs() {
        addr.to_string()
    } else {
        "cliente".to_string()
    }
}
