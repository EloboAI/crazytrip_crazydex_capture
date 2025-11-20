use serde::{Deserialize, Serialize};
use base64::{Engine as _, engine::general_purpose};

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

    /// Analyze image with Gemini Vision API
    pub async fn analyze_image(
        &self,
        image_bytes: &[u8],
    ) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        let base64_image = general_purpose::STANDARD.encode(image_bytes);

        let prompt = r#"Analiza esta imagen y proporciona información detallada en formato JSON con la siguiente estructura:
{
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
  "authenticity": "AUTHENTIC/REPLICA/UNCERTAIN"
}

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

Responde ÚNICAMENTE con el JSON, sin texto adicional."#;

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
        let url = format!("{}/{}:generateContent?key={}", self.endpoint, self.model, self.api_key);

        log::info!("Sending request to Gemini API: {}/{}:generateContent", self.endpoint, self.model);

        let response = self
            .http_client
            .post(&url)
            .json(&request_body)
            .send()
            .await?;

        let status = response.status();
        log::info!("Gemini API response status: {}", status);

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_else(|e| format!("Failed to read error body: {}", e));
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

        let vision_result: serde_json::Value = serde_json::from_str(json_str)
            .map_err(|e| format!("Failed to parse JSON: {}", e))?;

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
