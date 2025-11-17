# CrazyTrip Crazydex Capture Service

Microservicio de captura y análisis de imágenes con almacenamiento en S3 y análisis AI server-side usando Google Gemini Vision API.

## Características

- **Almacenamiento S3**: Presigned URLs para uploads directos, soporte para AWS S3 y MinIO
- **Análisis AI**: Análisis automático de imágenes con Google Gemini Vision API
- **Worker Background**: Procesamiento asíncrono de análisis pendientes
- **API REST**: Endpoints completos para CRUD de capturas
- **Sincronización**: Soporte para sync desde dispositivos móviles
- **Offline-first**: Diseñado para apps con cache local y sincronización diferida

## Requisitos

- Rust 1.70+
- PostgreSQL 14+
- AWS S3 o MinIO
- Google Gemini API Key

## Instalación

### 1. Clonar y configurar

```bash
cd crazytrip_crazydex_capture
cp .env.example .env
# Editar .env con tus credenciales
```

### 2. Configurar PostgreSQL

```bash
# Crear base de datos
createdb crazytrip_captures

# O usando psql
psql -U postgres
CREATE DATABASE crazytrip_captures;
\q
```

### 3. Configurar variables de entorno

Edita `.env` con tus credenciales:

```env
DATABASE_URL=postgresql://postgres:password@127.0.0.1:5432/crazytrip_captures

# AWS S3
AWS_ACCESS_KEY_ID=your_access_key
AWS_SECRET_ACCESS_KEY=your_secret_key
S3_BUCKET=crazytrip-captures

# Google Gemini
GEMINI_API_KEY=your_gemini_api_key_here
```

### 4. Compilar y ejecutar

```bash
# Desarrollo
cargo run

# Producción
cargo build --release
./target/release/crazytrip-crazydex-capture
```

## Desarrollo local con MinIO (alternativa a S3)

```bash
# Usar Docker para MinIO
docker run -d \
  -p 9000:9000 \
  -p 9001:9001 \
  --name minio \
  -e "MINIO_ROOT_USER=minioadmin" \
  -e "MINIO_ROOT_PASSWORD=minioadmin" \
  quay.io/minio/minio server /data --console-address ":9001"

# Actualizar .env
S3_ENDPOINT=http://localhost:9000
AWS_ACCESS_KEY_ID=minioadmin
AWS_SECRET_ACCESS_KEY=minioadmin
```

## API Endpoints

### Health Check
```bash
GET /api/v1/health
```

### Presigned Upload URL
```bash
POST /api/v1/uploads/presign
Content-Type: application/json

{
  "filename": "photo.jpg",
  "content_type": "image/jpeg"
}
```

### Create Capture
```bash
POST /api/v1/captures
Content-Type: application/json

{
  "image_url": "https://bucket.s3.amazonaws.com/...",
  "device_local_id": "uuid-local",
  "vision_result": {...},
  "category": "LANDMARK",
  "confidence": 0.95,
  "location": {"latitude": 10.0, "longitude": -84.0}
}
```

### List Captures
```bash
GET /api/v1/captures?page=1&limit=20
```

### Get Capture
```bash
GET /api/v1/captures/{id}
```

### Update Capture
```bash
PATCH /api/v1/captures/{id}
Content-Type: application/json

{
  "tags": ["tag1", "tag2"],
  "category": "NATURE"
}
```

### Delete Capture
```bash
DELETE /api/v1/captures/{id}
```

### Sync Upload (batch)
```bash
POST /api/v1/sync/upload
Content-Type: application/json

{
  "captures": [
    {
      "device_local_id": "local-uuid-1",
      "image_url": "https://...",
      "vision_result": {...},
      "timestamp": "2025-11-16T10:00:00Z"
    }
  ]
}
```

## Testing

```bash
# Unit tests
cargo test

# Integration test manual
curl http://localhost:8081/api/v1/health
```

## Licencia

Privado - EloboAI
sservicio para capturar los elementos de crazydex
