use rustls_pemfile::{certs, pkcs8_private_keys};
use std::fs::File;
use std::io::{self, BufReader};
use std::sync::Arc;
use tokio_rustls::rustls::{self, ClientConfig, RootCertStore, ServerConfig};
use tokio_rustls::{TlsAcceptor, TlsConnector};

/// Carrega certificados e chave privada, retorna um TlsAcceptor configurado.
///
/// # Argumentos
/// - `cert_path`: caminho para o arquivo do certificado (ex: "certs/cert.pem")
/// - `key_path`: caminho para a chave privada (ex: "certs/key.pem")
///
/// # Retorno
/// Um `TlsAcceptor` pronto para envolver conexões TCP aceitas.
pub fn load_tls_acceptor(cert_path: &str, key_path: &str) -> io::Result<TlsAcceptor> {
    // Lê os certificados
    let cert_file = File::open(cert_path).map_err(|e| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("Certificado não encontrado em {}: {}", cert_path, e),
        )
    })?;
    let mut cert_reader = BufReader::new(cert_file);
    let cert_chain: Vec<rustls::pki_types::CertificateDer> = certs(&mut cert_reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Erro ao ler certificados: {}", e),
            )
        })?;

    if cert_chain.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Nenhum certificado válido encontrado",
        ));
    }

    // Lê a chave privada
    let key_file = File::open(key_path).map_err(|e| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("Chave privada não encontrada em {}: {}", key_path, e),
        )
    })?;
    let mut key_reader = BufReader::new(key_file);
    let mut keys = pkcs8_private_keys(&mut key_reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Erro ao ler chave privada: {}", e),
            )
        })?;

    if keys.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Nenhuma chave privada válida encontrada",
        ));
    }

    let private_key = rustls::pki_types::PrivateKeyDer::Pkcs8(keys.remove(0));

    // Cria o ServerConfig do rustls
    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, private_key)
        .map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Erro ao configurar TLS: {}", e),
            )
        })?;

    Ok(TlsAcceptor::from(Arc::new(config)))
}

/// Carrega um TlsConnector para o cliente, opcionalmente com validação de certificados.
///
/// # Argumentos
/// - `insecure`: se true, aceita certificados autoassinados sem validação
/// - `ca_cert_path`: caminho opcional para CA personalizada (ex: "certs/cert.pem")
///
/// # Retorno
/// Um `TlsConnector` para conectar ao servidor TLS.
pub fn load_tls_connector(insecure: bool, ca_cert_path: Option<&str>) -> io::Result<TlsConnector> {
    let mut root_store = RootCertStore::empty();

    if insecure {
        // Modo inseguro: usa validação customizada que aceita tudo
        // ATENÇÃO: apenas para desenvolvimento!
        let config = ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoVerifier))
            .with_no_client_auth();

        return Ok(TlsConnector::from(Arc::new(config)));
    }

    // Modo seguro: carrega CA do sistema ou do arquivo especificado
    if let Some(ca_path) = ca_cert_path {
        let ca_file = File::open(ca_path).map_err(|e| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("Certificado CA não encontrado em {}: {}", ca_path, e),
            )
        })?;
        let mut ca_reader = BufReader::new(ca_file);
        let ca_certs: Vec<rustls::pki_types::CertificateDer> = certs(&mut ca_reader)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                io::Error::new(io::ErrorKind::InvalidData, format!("Erro ao ler CA: {}", e))
            })?;

        for cert in ca_certs {
            root_store.add(cert).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Erro ao adicionar CA: {}", e),
                )
            })?;
        }
    } else {
        // Usa certificados do sistema
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    }

    let config = ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    Ok(TlsConnector::from(Arc::new(config)))
}

// Verificador customizado que aceita qualquer certificado (APENAS PARA DESENVOLVIMENTO!)
#[derive(Debug)]
struct NoVerifier;

impl rustls::client::danger::ServerCertVerifier for NoVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ED25519,
        ]
    }
}
