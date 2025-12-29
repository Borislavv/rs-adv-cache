# Grafana Setup для AdvCache

Этот документ описывает, как настроить и использовать Grafana для визуализации метрик AdvCache.

## Архитектура

```
AdvCache (порт 8020) 
    ↓ /metrics endpoint
Prometheus (порт 9090) - собирает метрики
    ↓
Grafana (порт 3000) - визуализирует метрики
```

## Быстрый старт

### 1. Запуск сервисов

```bash
# Запустить все сервисы (Prometheus, Grafana)
docker-compose up -d prometheus grafana

# Проверить статус
docker compose ps
```

### 2. Проверка работы

#### Prometheus
- URL: http://localhost:9090
- Проверить targets: http://localhost:9090/targets
- Должен быть виден target `advcache` со статусом UP

#### Grafana
- URL: http://localhost:3000
- Логин: `admin`
- Пароль: `admin` (при первом входе предложит сменить)

### 3. Доступ к метрикам

#### Проверка метрик AdvCache
Убедитесь, что приложение AdvCache запущено и доступно на порту 8020:

```bash
# Проверить, что метрики доступны
curl http://localhost:8020/metrics
```

Должны быть видны метрики в формате Prometheus, например:
```
# HELP cache_hits Total number of cache hits
# TYPE cache_hits counter
cache_hits 1234

# HELP cache_misses Total number of cache misses
# TYPE cache_misses counter
cache_misses 567
```

**Примечание**: Если метрики не отображаются, убедитесь, что Prometheus exporter правильно инициализирован в коде приложения. Метрики должны быть записаны через `metrics::counter!()`, `metrics::gauge!()` и т.д. для их появления в `/metrics` endpoint.

## Настройка Prometheus

### Конфигурация

Файл конфигурации: `cfg/prometheus/prometheus.yml`

```yaml
scrape_configs:
  - job_name: 'advcache'
    static_configs:
      - targets: ['host.docker.internal:8020']
```

**Важно**: 
- `host.docker.internal` работает на Docker Desktop (Mac/Windows)
- На Linux может потребоваться использовать `172.17.0.1:8020` или настроить network_mode

### Если приложение запущено в Docker

Если AdvCache также запущен в docker-compose, измените target на:

```yaml
targets: ['advcache:8020']  # имя сервиса из docker-compose
```

И добавьте `advcache` в ту же сеть:

```yaml
networks:
  - advcache_network
```

## Настройка Grafana

### Datasource

Datasource для Prometheus настраивается автоматически через provisioning:
- Файл: `cfg/grafana/provisioning/datasources/prometheus.yml`
- URL: `http://prometheus:9090`

### Dashboard

Dashboard автоматически импортируется из:
- Файл: `cfg/grafana/dashboards/advcache-dashboard.json`

### Ручная настройка (если нужно)

1. Зайти в Grafana: http://localhost:3000
2. Configuration → Data Sources → Add data source
3. Выбрать Prometheus
4. URL: `http://prometheus:9090`
5. Save & Test

## Доступные метрики

Основные метрики, экспортируемые AdvCache:

### Cache метрики
- `cache_hits` - количество cache hits (counter)
- `cache_misses` - количество cache misses (counter)
- `cache_length` - текущее количество записей в кеше (gauge)
- `cache_memory_usage` - использование памяти кешем в байтах (gauge)

### Request метрики
- `total` - общее количество запросов (counter)
- `rps` - запросов в секунду (gauge)
- `proxies` - количество проксированных запросов (counter)
- `errors` - количество ошибок (counter)
- `panics` - количество паник (counter)

### Latency метрики
- `avg_duration_ns` - средняя длительность запроса в наносекундах (gauge)
- `avg_cache_duration_ns` - средняя длительность cache операций (gauge)
- `avg_proxy_duration_ns` - средняя длительность proxy операций (gauge)
- `avg_error_duration_ns` - средняя длительность обработки ошибок (gauge)

### Eviction метрики
- `soft_evicted_total_items` - количество мягких evictions (counter)
- `soft_evicted_total_bytes` - байт при мягких evictions (counter)
- `hard_evicted_total_items` - количество жестких evictions (counter)
- `hard_evicted_total_bytes` - байт при жестких evictions (counter)

### Admission метрики
- `admission_allowed` - разрешено admission control (counter)
- `admission_not_allowed` - отклонено admission control (counter)

### Refresh метрики
- `refresh_updated` - обновлено через refresh (counter)
- `refresh_errors` - ошибок при refresh (counter)
- `refresh_hits` - hits при refresh (counter)
- `refresh_miss` - misses при refresh (counter)

### Status метрики
- `is_bypass_active` - активен ли bypass (gauge: 1.0 = да, 0.0 = нет)
- `is_compression_active` - активна ли компрессия (gauge)
- `is_admission_active` - активен ли admission control (gauge)
- `is_traces_active` - активны ли traces (gauge)

## Использование Dashboard

### Просмотр метрик

1. Открыть Grafana: http://localhost:3000
2. Войти (admin/admin)
3. Перейти в Dashboards → AdvCache Metrics
4. Вы увидите панели с:
   - Cache hits/misses rate
   - Request rate (RPS)
   - Latency metrics
   - Cache size and memory usage
   - Eviction statistics
   - И другие метрики

### Настройка временного диапазона

В правом верхнем углу можно выбрать:
- Last 5 minutes
- Last 15 minutes
- Last 1 hour
- Custom range

### Запросы в Prometheus

Можно использовать Prometheus UI для прямых запросов:

1. Открыть http://localhost:9090
2. Ввести запрос, например:
   ```
   rate(cache_hits[1m])
   rate(cache_misses[1m])
   cache_length
   ```

## Troubleshooting

### Prometheus не видит метрики

1. Проверить, что AdvCache запущен:
   ```bash
   curl http://localhost:8020/metrics
   ```

2. Проверить targets в Prometheus:
   - http://localhost:9090/targets
   - Должен быть статус UP

3. Проверить логи Prometheus:
   ```bash
   docker-compose logs prometheus
   ```

### Grafana не видит Prometheus

1. Проверить, что Prometheus запущен:
   ```bash
   docker-compose ps prometheus
   curl http://localhost:9090/api/v1/status/config
   ```

2. Проверить datasource в Grafana:
   - Configuration → Data Sources → Prometheus
   - Нажать "Save & Test"

3. Проверить логи Grafana:
   ```bash
   docker-compose logs grafana
   ```

### Метрики не отображаются в Dashboard

1. Проверить, что метрики есть в Prometheus:
   - http://localhost:9090/graph
   - Ввести запрос: `cache_hits`

2. Проверить временной диапазон в Grafana (метрики могут быть старыми)

3. Проверить запросы в панелях dashboard (могут использовать старые имена метрик)

### Проблемы с network на Linux

Если `host.docker.internal` не работает на Linux:

1. Использовать IP хоста:
   ```yaml
   targets: ['172.17.0.1:8020']  # или другой IP docker bridge
   ```

2. Или использовать host network mode для Prometheus:
   ```yaml
   network_mode: host
   ```

## Остановка сервисов

```bash
# Остановить все
docker-compose down

# Остановить с удалением volumes (удалит данные Prometheus и Grafana)
docker-compose down -v
```

## Дополнительная информация

- Prometheus документация: https://prometheus.io/docs/
- Grafana документация: https://grafana.com/docs/
- Метрики AdvCache описаны в `METRICS.md`
