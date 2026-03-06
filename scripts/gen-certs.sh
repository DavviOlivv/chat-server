#!/usr/bin/env bash
set -euo pipefail

# Gera certificado autoassinado para localhost em certs/
mkdir -p certs

openssl req -x509 -newkey rsa:4096 -keyout certs/key.pem -out certs/cert.pem -sha256 -days 365 -nodes \
  -subj "/C=BR/ST=State/L=City/O=ChatOrg/CN=localhost"

chmod 600 certs/key.pem
ls -l certs

echo "Certificados gerados em certs/ (cert.pem, key.pem)" 
