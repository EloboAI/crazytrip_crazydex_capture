use base64::{engine::general_purpose, Engine as _};
use chrono::{DateTime, Datelike, Timelike, Utc};
use serde::{Deserialize, Serialize};

use crate::config::AIConfig;

#[derive(Debug, Serialize)]
struct GeminiRequest {
    contents: Vec<Content>,
}

#[derive(Debug, Serialize)]
struct Content {
    parts: Vec<Part>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum Part {
    Text { text: String },
    InlineData { inline_data: InlineData },
}

#[derive(Debug, Serialize)]
struct InlineData {
    mime_type: String,
    data: String,
}

#[derive(Debug, Deserialize)]
struct GeminiResponse {
    candidates: Vec<Candidate>,
}

#[derive(Debug, Deserialize)]
struct Candidate {
    content: ResponseContent,
}

#[derive(Debug, Deserialize)]
struct ResponseContent {
    parts: Vec<ResponsePart>,
}

#[derive(Debug, Deserialize)]
struct ResponsePart {
    text: String,
}

pub struct AIService {
    api_key: String,
    endpoint: String,
    model: String,
    http_client: reqwest::Client,
}

impl AIService {
    pub fn new(config: &AIConfig) -> Self {
        Self {
            api_key: config.gemini_api_key.clone(),
            endpoint: config.gemini_endpoint.clone(),
            model: config.gemini_model.clone(),
            http_client: reqwest::Client::new(),
        }
    }

    /// Calculate sun position (azimuth and elevation) for given location and time
    /// Returns (azimuth, elevation, is_daylight) where:
    /// - azimuth: 0° = North, 90° = East, 180° = South, 270° = West
    /// - elevation: angle above horizon (-90° to 90°)
    /// - is_daylight: true if sun is above horizon
    fn calculate_sun_position(lat: f64, lon: f64, timestamp: &DateTime<Utc>) -> (f64, f64, bool) {
        // Convert to Julian Day
        let y = timestamp.year() as f64;
        let m = timestamp.month() as f64;
        let d = timestamp.day() as f64;
        let h = timestamp.hour() as f64 + timestamp.minute() as f64 / 60.0;

        let jd = 367.0 * y - (7.0 * (y + ((m + 9.0) / 12.0).floor()) / 4.0).floor()
            + (275.0 * m / 9.0).floor()
            + d
            + 1721013.5
            + h / 24.0;

        // Days since J2000.0
        let n = jd - 2451545.0;

        // Mean longitude of sun
        let l = (280.460 + 0.9856474 * n) % 360.0;

        // Mean anomaly
        let g = ((357.528 + 0.9856003 * n) % 360.0).to_radians();

        // Ecliptic longitude
        let lambda = (l + 1.915 * g.sin() + 0.020 * (2.0 * g).sin()).to_radians();

        // Obliquity of ecliptic
        let epsilon = (23.439 - 0.0000004 * n).to_radians();

        // Right ascension and declination
        let ra = lambda.cos().atan2(epsilon.cos() * lambda.sin());
        let dec = (epsilon.sin() * lambda.sin()).asin();

        // Greenwich mean sidereal time
        let gmst = (280.460 + 360.98564724 * n) % 360.0;

        // Local sidereal time
        let lst = ((gmst + lon) % 360.0).to_radians();

        // Hour angle
        let ha = lst - ra;

        let lat_rad = lat.to_radians();

        // Elevation (altitude)
        let elevation = (lat_rad.sin() * dec.sin() + lat_rad.cos() * dec.cos() * ha.cos()).asin();

        // Azimuth
        let azimuth =
            (dec.sin() - lat_rad.sin() * elevation.sin()) / (lat_rad.cos() * elevation.cos());
        let azimuth = azimuth.acos();
        let azimuth = if ha.sin() > 0.0 {
            2.0 * std::f64::consts::PI - azimuth
        } else {
            azimuth
        };

        let elevation_deg = elevation.to_degrees();
        let azimuth_deg = azimuth.to_degrees();
        let is_daylight = elevation_deg > -6.0; // Civil twilight threshold

        (azimuth_deg, elevation_deg, is_daylight)
    }

    /// Analyze image with Gemini Vision API with optional geographic context
    pub async fn analyze_image(
        &self,
        image_bytes: &[u8],
        location: Option<&serde_json::Value>,
        location_info: Option<&serde_json::Value>,
        orientation: Option<&serde_json::Value>,
        timestamp: Option<&DateTime<Utc>>,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        let base64_image = general_purpose::STANDARD.encode(image_bytes);

        // Build geographic and temporal context string
        let mut context_parts = Vec::new();

        // Extract coordinates for sun calculation
        let mut lat_lon: Option<(f64, f64)> = None;

        if let Some(loc) = location {
            if let (Some(lat), Some(lon)) = (loc.get("latitude"), loc.get("longitude")) {
                context_parts.push(format!("📍 Coordenadas GPS: {}, {}", lat, lon));
                if let (Some(lat_f), Some(lon_f)) = (lat.as_f64(), lon.as_f64()) {
                    lat_lon = Some((lat_f, lon_f));
                }
            }
        }

        if let Some(loc_info) = location_info {
            if let Some(country) = loc_info.get("country").and_then(|v| v.as_str()) {
                context_parts.push(format!("🌍 País: {}", country));
            }
            if let Some(city) = loc_info.get("city").and_then(|v| v.as_str()) {
                context_parts.push(format!("🏙️ Ciudad: {}", city));
            }
            if let Some(place) = loc_info.get("placeName").and_then(|v| v.as_str()) {
                context_parts.push(format!("📌 Lugar: {}", place));
            }
        }

        // Add temporal context with sun position
        if let (Some(ts), Some((lat, lon))) = (timestamp, lat_lon) {
            let (sun_azimuth, sun_elevation, is_daylight) =
                Self::calculate_sun_position(lat, lon, ts);

            let local_time = format!("🕐 Hora captura (UTC): {}", ts.format("%Y-%m-%d %H:%M:%S"));
            let sun_info = format!(
                "☀️ Posición solar: Azimuth {:.0}°, Elevación {:.1}° ({})",
                sun_azimuth,
                sun_elevation,
                if is_daylight { "DÍA" } else { "NOCHE" }
            );

            context_parts.push(local_time);
            context_parts.push(sun_info);

            // Add sun direction interpretation
            let sun_direction = if sun_azimuth < 45.0 || sun_azimuth >= 315.0 {
                "Norte"
            } else if sun_azimuth < 135.0 {
                "Este"
            } else if sun_azimuth < 225.0 {
                "Sur"
            } else {
                "Oeste"
            };
            context_parts.push(format!("🌅 Sol hacia el: {}", sun_direction));
        }

        if let Some(orient) = orientation {
            if let Some(bearing) = orient.get("bearing").and_then(|v| v.as_f64()) {
                context_parts.push(format!(
                    "🧭 Dirección cámara: {:.0}° ({})",
                    bearing,
                    orient
                        .get("cardinalDirection")
                        .and_then(|v| v.as_str())
                        .unwrap_or("N/A")
                ));
            }
        }

        let geographic_context = if context_parts.is_empty() {
            String::new()
        } else {
            format!("\n\nCONTEXTO GEOGRÁFICO Y TEMPORAL DE LA CAPTURA:\n{}\n\nUSA ESTE CONTEXTO PARA VALIDAR AUTENTICIDAD:\n- Verificar coherencia entre iluminación de la imagen y posición solar esperada\n- Si es NOCHE pero imagen muestra sol brillante → probablemente SCREEN_PHOTO\n- Si dirección de sombras no coincide con posición solar → SCREEN_PHOTO o editada\n- Comparar dirección de cámara con posición del sol para validar iluminación\n- Diferenciar entre originales y réplicas basándose en ubicación GPS\n- Detectar fotos de pantallas por inconsistencias temporales/lumínicas\n", 
                context_parts.join("\n"))
        };

        let prompt = format!(
            r#"Analiza esta imagen y proporciona información detallada en formato JSON con la siguiente estructura:
{{
  "name": "Nombre del lugar, monumento, animal o concepto principal",
  "type": "LUGAR/MONUMENTO/NATURALEZA/ANIMAL/OBJETO/OTRO",
  "category": "Categoría específica (LANDMARK/NATURE/WILDLIFE/FOOD/ARCHITECTURE/ART/CULTURE/TRANSPORTATION)",
  "tags": ["descriptive", "searchable", "keywords"],
  "description": "Descripción detallada de lo que se ve",
  "rarity": "COMMON/UNCOMMON/RARE/VERY_RARE/LEGENDARY",
  "confidence": 0.95,
  "difficulty": "EASY/MEDIUM/HARD/EXPERT",
  "specificity_level": "Nivel de especificidad de la identificación",
  "broader_context": "Contexto más amplio o información adicional",
  "encounter_rarity": "Qué tan difícil es encontrar esto aquí",
  "authenticity": "AUTHENTIC/REPLICA/SCREEN_PHOTO/UNCERTAIN",
  "geographic_match": true,
  "verified": true,
  "authenticity_reasoning": "Explicación de por qué es auténtico, réplica o foto de pantalla basándose en ubicación y contexto",
  "verification_reasoning": "Explicación de por qué está verificado o no basándose en conocimiento geográfico del hábitat/ubicación natural"
}}{}

REGLAS para tags (3-8 tags por imagen):
- Incluir: características físicas, contexto cultural, época, materiales, colores dominantes, ubicación geográfica
- Formato: lowercase, sin acentos, singular, en español
- Ejemplos: ["volcanico", "unesco", "colonial", "turquesa", "cascada", "tropical"]
- Evitar: duplicar el nombre exacto o la categoría

REGLAS para difficulty:
- EASY: Muy común, fácil de encontrar, visible desde lejos
- MEDIUM: Requiere buscar un poco, moderadamente común
- HARD: Difícil de encontrar, requiere esfuerzo o conocimiento local
- EXPERT: Extremadamente raro, requiere condiciones especiales o permiso

REGLAS para authenticity (MUY IMPORTANTE - USA EL CONTEXTO GEOGRÁFICO Y TEMPORAL):
- AUTHENTIC: Objeto/lugar real capturado en su ubicación original. Verifica:
  * Ubicación GPS coincide con donde debería estar el objeto
  * Iluminación coherente con hora del día y posición solar calculada
  * Sombras apuntan en dirección correcta según posición del sol
  * Si es NOCHE (elevación solar negativa), imagen debe ser nocturna
- REPLICA: Copia o réplica del objeto original en diferente ubicación (ej: Torre Eiffel en Las Vegas cuando GPS dice USA)
- SCREEN_PHOTO: Foto de una pantalla, fotografía impresa, póster o imagen digital. Indicadores críticos:
  * Píxeles visibles, patrón de matriz de pantalla
  * Brillo artificial o reflexiones de vidrio/pantalla
  * Iluminación inconsistente con hora y posición solar (ej: sol brillante cuando es noche)
  * Sombras en dirección imposible para la ubicación/hora
  * Marco de foto, borde de pantalla o dispositivo visible
  * Calidad de imagen degradada (foto de foto)
- UNCERTAIN: No hay suficiente información para determinar

REGLAS para geographic_match:
- true: La ubicación GPS es coherente con el objeto identificado Y la iluminación coincide con hora/posición solar
- false: La ubicación GPS NO coincide O hay inconsistencias temporales graves
- null: No hay suficiente contexto geográfico para determinar

REGLAS para verified (VALIDACIÓN GEOGRÁFICA ESTRICTA):
Este campo certifica que el objeto/animal/lugar es REALMENTE observable desde la ubicación GPS proporcionada.

✅ verified = true SOLO SI SE CUMPLEN TODAS:
1. authenticity = "AUTHENTIC" (no réplica, no screen_photo)
2. geographic_match = true (GPS coincide)
3. Para ANIMALES: El animal existe naturalmente en esa región O está en un zoo/santuario CONOCIDO en esa ubicación específica
   - Ejemplo: León en Kenia (Masai Mara) → verified=true
   - Ejemplo: Elefante en Costa Rica (ubicación aleatoria) → verified=false
   - Ejemplo: Elefante en "Zoo Simón Bolívar, San José, CR" → verified=true (zoo conocido)
4. Para LUGARES/MONUMENTOS: El lugar existe en esas coordenadas GPS exactas
   - Ejemplo: Torre Eiffel en París (48.858°N, 2.294°E) → verified=true
   - Ejemplo: Torre Eiffel en Las Vegas → verified=false (réplica)
5. Para NATURALEZA: El fenómeno natural es posible en esa ubicación geográfica
   - Ejemplo: Volcán Arenal en La Fortuna, CR → verified=true
   - Ejemplo: Glaciar en Ecuador → verified=false (geográficamente improbable)
6. Iluminación y hora coherentes (no foto de pantalla)

❌ verified = false SI CUALQUIERA:
- Es una réplica en ubicación diferente
- Es foto de pantalla/póster
- Animal fuera de su hábitat natural y NO en zoo conocido
- Ubicación GPS imposible para el objeto
- Inconsistencias temporales (hora vs iluminación)

⚠️ IMPORTANTE: No asumas que hay zoos o santuarios a menos que el contexto geográfico mencione un lugar específico conocido.

Responde ÚNICAMENTE con el JSON, sin texto adicional."#,
            geographic_context
        );

        let request_body = GeminiRequest {
            contents: vec![Content {
                parts: vec![
                    Part::Text {
                        text: prompt.to_string(),
                    },
                    Part::InlineData {
                        inline_data: InlineData {
                            mime_type: "image/jpeg".to_string(),
                            data: base64_image,
                        },
                    },
                ],
            }],
        };

        // Build model generateContent URL: {endpoint}/{model}:generateContent?key={API_KEY}
        let url = format!(
            "{}/{}:generateContent?key={}",
            self.endpoint, self.model, self.api_key
        );

        log::info!(
            "Sending request to Gemini API: {}/{}:generateContent",
            self.endpoint,
            self.model
        );

        let response = self
            .http_client
            .post(&url)
            .json(&request_body)
            .send()
            .await?;

        let status = response.status();
        log::info!("Gemini API response status: {}", status);

        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|e| format!("Failed to read error body: {}", e));
            log::error!("Gemini API error response: {}", error_text);
            return Err(format!("Gemini API error ({}): {}", status, error_text).into());
        }

        let gemini_response: GeminiResponse = response.json().await?;

        if gemini_response.candidates.is_empty() {
            return Err("No candidates returned from Gemini".into());
        }

        if gemini_response.candidates[0].content.parts.is_empty() {
            return Err("No parts in candidate response".into());
        }

        let text = &gemini_response.candidates[0].content.parts[0].text;
        log::info!("Gemini raw response text: {}", text);

        // Parse JSON from response
        let json_start = text.find('{').unwrap_or(0);
        let json_end = text.rfind('}').unwrap_or(text.len());

        if json_start >= json_end {
            log::error!("No valid JSON found in response: {}", text);
            return Err("No valid JSON in Gemini response".into());
        }

        let json_str = &text[json_start..=json_end];
        log::info!("Extracted JSON: {}", json_str);

        let vision_result: serde_json::Value =
            serde_json::from_str(json_str).map_err(|e| format!("Failed to parse JSON: {}", e))?;

        log::info!("Image analyzed successfully");
        Ok(vision_result)
    }

    /// Extract category and confidence from vision result
    pub fn extract_metadata(vision_result: &serde_json::Value) -> (String, f64) {
        let category = vision_result
            .get("category")
            .and_then(|v| v.as_str())
            .unwrap_or("UNKNOWN")
            .to_string();

        let confidence = vision_result
            .get("confidence")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        (category, confidence)
    }
}
