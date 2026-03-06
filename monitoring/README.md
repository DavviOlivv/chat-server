# Monitoring — Instruções rápidas

Este diretório contém arquivos de exemplo para rodar Prometheus contra o `chat_server` localmente ou em Docker/Kubernetes:

- `prometheus.yml` — exemplo mínimo de configuração Prometheus (aponta para `localhost:9090`).
- `alert_rules.yml` — regras de alerta (ex.: `ChatSendFullHigh` para `chat_send_full_total`).

Uso com Docker Compose
----------------------
1. Monte os arquivos no diretório `monitoring/` do host.
2. Exemplo de `docker-compose` já presente no `README.md` — ele monta `prometheus.yml` e `alert_rules.yml` no container Prometheus.
3. Para iniciar localmente:

```bash
# construa a imagem do servidor (ou use a imagem existente)
make build
# execute via docker-compose (assumindo docker-compose.yml apontando para monitoring/prometheus.yml)
docker compose up -d
Uso com Docker Compose

4. Verifique o endpoint de métricas do `chat_server`:

```bash
curl http://localhost:9090/metrics
```

5. Recarregue regras do Prometheus sem reiniciar (se configurado):

```bash
curl -X POST http://localhost:9091/-/reload
```

(ajuste porta se seu Prometheus expõe em outra porta; no `docker-compose` de exemplo usamos `9091` para acessar o Prometheus web UI)

Uso com Kubernetes (Prometheus Operator)
--------------------------------------
1. Crie um `Service` que exponha a porta `9090` do seu `chat_server` (ex.: `chat-server-metrics`).
2. Crie um `ServiceMonitor` (ou `PodMonitor`) que selecione o `Service` e configure `path: /metrics` e `port: metrics`.
3. Coloque `alert_rules.yml` na configuração do Prometheus (via Prometheus CR `spec.ruleSelector` ou ConfigMap, depende do seu operador).

Exemplo mínimo de `ServiceMonitor` (veja README principal para um snippet):

```yaml
apiVersion: monitoring.coreos.com/v1
kind: ServiceMonitor
Monitoramento: quickstart (docker-compose)
-----------------------------------------

1. Gere o arquivo `prometheus.yml` a partir do template (substitua o TOKEN):

```bash
export METRICS_BEARER_TOKEN=secret123
envsubst < monitoring/prometheus/prometheus.yml.template > monitoring/prometheus/prometheus.yml
```

2. Inicie Prometheus + Grafana:

```bash
docker compose -f docker-compose.monitoring.yml up -d
```

3. Acesse Grafana em `http://localhost:3000` (usuário `admin`, senha `admin`) e você verá o dashboard `Chat Serve Metrics` provisionado automaticamente.

Observação: no Linux, se o `chat_server` roda no host, troque `host.docker.internal` em `prometheus.yml.template` para o IP do host ou hostname apropriado.

Regras de alerta
----------------
Incluí um conjunto básico de regras em `monitoring/alert_rules.yml`:

- `HighUnauthorizedMetricsAccess`: dispara quando `chat_metrics_unauthorized_total` aumenta em um intervalo de 5 minutos (sinal de acessos não autorizados ao `/metrics`).
- `ChatSendFullHigh` (ou `HighSendFullRate`): detecta aumento rápido em `chat_send_full_total` (possível backpressure / clientes lentos).

Essas regras já são carregadas automaticamente pelo `prometheus.yml` gerado a partir do template (montado em `/etc/prometheus/alert_rules.yml` no container). Para recarregar regras manualmente:

```bash
curl -X POST http://localhost:9090/-/reload
```

Você pode editar `monitoring/alert_rules.yml` para ajustar thresholds e severidades.

Alertmanager
------------
Incluí uma configuração mínima do Alertmanager em `monitoring/alertmanager/alertmanager.yml` com placeholders para Slack e email.
O `docker-compose.monitoring.yml` agora sobe o Alertmanager na porta `9093` e o Prometheus envia alertas para `alertmanager:9093`.

Para ativar notificações reais:

1. Edite `monitoring/alertmanager/alertmanager.yml` e descomente/configure a seção `slack_configs` (com webhook) ou `email_configs` (configure `global.smtp_*`).
2. Reinicie o stack ou recarregue o Alertmanager (dependendo da sua configuração):

```bash
docker compose -f docker-compose.monitoring.yml restart alertmanager
```

3. Recarregue regras do Prometheus se necessário:

```bash
curl -X POST http://localhost:9090/-/reload
```

Exemplo mínimo de Slack config (no arquivo `monitoring/alertmanager/alertmanager.yml`):

```yaml
receivers:
  - name: 'slack-or-email'
    slack_configs:
      - api_url: 'https://hooks.slack.com/services/YOUR_WORKSPACE/YOUR_CHANNEL/YOUR_WEBHOOK_TOKEN'
        channel: '#alerts'
```

Observação de segurança: mantenha webhooks e credenciais fora do repositório (use secrets/variables em CI ou volums seguras em produção).

Gerar Alertmanager automaticamente via `start.sh`
------------------------------------------------
O `monitoring/start.sh` agora também gera `monitoring/alertmanager/alertmanager.yml` a partir do template se o arquivo `monitoring/alertmanager/alertmanager.yml.template` existir.

Você pode fornecer um webhook do Slack com a variável `SLACK_WEBHOOK_URL` antes de executar `start.sh`:

```bash
export METRICS_BEARER_TOKEN=secret123
export SLACK_WEBHOOK_URL="https://hooks.slack.com/services/YOUR_WORKSPACE/YOUR_CHANNEL/YOUR_WEBHOOK_TOKEN"
./monitoring/start.sh
```

O script usa `envsubst` para substituir as variáveis no template (garanta que `envsubst` esteja instalado).
metadata:
  name: chat-server-monitor
spec:
  selector:
    matchLabels:
      app: chat-server
  endpoints:
    - port: metrics
+     path: /metrics
+     interval: 15s
```

Boas práticas
------------
- Configure regras de retenção e regras de gravação conforme sua carga.
- Em produção, monte `prometheus.yml` e `alert_rules.yml` via ConfigMaps e gerencie a recarga automática.
- Crie painéis no Grafana que usem `chat_send_full_total` e outras métricas para visualizar tendências e gerar alertas.

Se quiser, eu posso gerar também um `docker-compose.yml` completo de exemplo no diretório `monitoring/` com Prometheus e Grafana pré-configurados.

     path: /metrics
     interval: 15s
Painéis sugeridos (PromQL)
--------------------------

Aqui estão consultas e painéis rápidos que você pode adicionar ao Grafana:

1) Sessões ativas (Stat / Gauge)
- Query: `chat_active_sessions`

2) Sessões expiradas (por janela)
- Query: `increase(chat_expired_sessions_total[5m])`

3) Tentativas não autorizadas ao `/metrics`
- Query: `increase(chat_metrics_unauthorized_total[5m])`

4) Falhas por canal cheio
- Query: `increase(chat_send_full_total[5m])`

Alertas rápidos sugeridos
------------------------
- `HighUnauthorizedMetricsAccess`: `increase(chat_metrics_unauthorized_total[5m]) > 0` (aviso)
- `HighSendFullRate`: `increase(chat_send_full_total[5m]) > 10` (alerta crítico — muitos clientes lentos)

Notas
-----
- Ajuste as janelas (`[5m]`) conforme o intervalo de scrape do seu Prometheus.
- Exporte painéis do Grafana e salve em `monitoring/grafana/` para versionamento e reuso.

