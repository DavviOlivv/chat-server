#!/usr/bin/env bash
set -euo pipefail

# monitoring/start.sh
# Automação para subir Prometheus + Grafana para o projeto chat-serve

if [[ -z "${METRICS_BEARER_TOKEN:-}" ]]; then
  echo "ERROR: METRICS_BEARER_TOKEN não está definido. Exporte-o antes de rodar. Ex: export METRICS_BEARER_TOKEN=secret123"
  exit 1
fi

TEMPLATE=monitoring/prometheus/prometheus.yml.template
OUT=monitoring/prometheus/prometheus.yml
COMPOSE_FILE=docker-compose.monitoring.yml
AM_TEMPLATE=monitoring/alertmanager/alertmanager.yml.template
AM_OUT=monitoring/alertmanager/alertmanager.yml

if [[ ! -f "$TEMPLATE" ]]; then
  echo "ERROR: template $TEMPLATE não encontrado"
  exit 1
fi

echo "Gerando $OUT a partir do template..."
# Usa envsubst para injetar METRICS_BEARER_TOKEN
if ! command -v envsubst >/dev/null 2>&1; then
  echo "envsubst não encontrado. Instale 'gettext' (ex: apt install gettext -y) ou use outro método para substituir a variável.";
  exit 1
fi

envsubst < "$TEMPLATE" > "$OUT"
 # Se existir template do Alertmanager, gere-o também (aceita SLACK_WEBHOOK_URL e vars de SMTP)
 if [[ -f "$AM_TEMPLATE" ]]; then
   echo "Gerando $AM_OUT a partir do template de Alertmanager..."
   envsubst < "$AM_TEMPLATE" > "$AM_OUT"
   echo "Arquivo alertmanager gerado: $AM_OUT"
 fi
echo "Arquivo prometheus gerado: $OUT"

echo "Subindo Prometheus + Grafana com docker-compose..."
docker compose -f "$COMPOSE_FILE" up -d

echo
echo "---- Monitoramento iniciado ----"
echo "Grafana: http://localhost:3000  (user: admin / pass: admin)"
echo "Prometheus: http://localhost:9090"
echo "Se o chat_server roda no host, garanta que o prometheus.yml aponte para o host correto (veja monitoring/prometheus/prometheus.yml.template)"
echo "Para parar: docker compose -f $COMPOSE_FILE down"

echo "---------------------------------"

exit 0
