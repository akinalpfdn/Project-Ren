//! `web.weather` — current conditions and today's forecast from Open-Meteo.
//!
//! Open-Meteo is free and key-less, which suits Ren's "no cloud subscriptions"
//! ethos. Location resolution uses Open-Meteo's geocoding endpoint when the
//! user supplies a place name; when they don't, we fall back to the
//! `location` field in `AppConfig`.

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::config::AppConfig;
use crate::tools::{Tool, ToolError, ToolResult};

const GEOCODE_URL: &str = "https://geocoding-api.open-meteo.com/v1/search";
const FORECAST_URL: &str = "https://api.open-meteo.com/v1/forecast";

pub struct Weather {
    http: Arc<reqwest::Client>,
    default_location: Option<String>,
}

impl Weather {
    pub fn new(http: Arc<reqwest::Client>, config: &AppConfig) -> Self {
        Self {
            http,
            default_location: config.location.clone(),
        }
    }
}

#[async_trait]
impl Tool for Weather {
    fn name(&self) -> &str {
        "web.weather"
    }

    fn description(&self) -> &str {
        "Report the current weather and today's high/low for a city. If no location is given, \
         the user's configured default location is used."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "location": {
                    "type": "string",
                    "description": "City or place name. Leave empty to use the user's default."
                }
            },
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
        let requested = args
            .get("location")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);

        let query = requested.or_else(|| self.default_location.clone()).ok_or_else(|| {
            ToolError::MissingConfig {
                tool: self.name().into(),
                missing: "no 'location' argument and no default location configured".into(),
            }
        })?;

        let place = geocode(&self.http, &query)
            .await
            .map_err(|e| ToolError::execution(self.name(), e))?;

        let reading = fetch_current(&self.http, place.latitude, place.longitude)
            .await
            .map_err(|e| ToolError::execution(self.name(), e))?;

        let summary = format!(
            "{} in {} — {}°C, feels like {}°C. Today's range: {}°C to {}°C.",
            describe(reading.weather_code),
            place.label,
            reading.temperature.round() as i32,
            reading.apparent_temperature.round() as i32,
            reading.daily_low.round() as i32,
            reading.daily_high.round() as i32,
        );
        Ok(ToolResult::new(summary))
    }
}

#[derive(Debug)]
struct ResolvedPlace {
    label: String,
    latitude: f64,
    longitude: f64,
}

#[derive(Debug)]
struct CurrentReading {
    temperature: f64,
    apparent_temperature: f64,
    weather_code: u32,
    daily_high: f64,
    daily_low: f64,
}

async fn geocode(http: &reqwest::Client, query: &str) -> Result<ResolvedPlace, String> {
    #[derive(Deserialize)]
    struct GeocodeResp {
        results: Option<Vec<GeocodeHit>>,
    }
    #[derive(Deserialize)]
    struct GeocodeHit {
        name: String,
        latitude: f64,
        longitude: f64,
        country: Option<String>,
        admin1: Option<String>,
    }

    let resp: GeocodeResp = http
        .get(GEOCODE_URL)
        .query(&[("name", query), ("count", "1"), ("language", "en")])
        .send()
        .await
        .map_err(|e| format!("geocoding request failed: {}", e))?
        .error_for_status()
        .map_err(|e| format!("geocoding returned error: {}", e))?
        .json()
        .await
        .map_err(|e| format!("could not parse geocoding response: {}", e))?;

    let hit = resp
        .results
        .and_then(|mut v| v.drain(..).next())
        .ok_or_else(|| format!("no place found for '{}'", query))?;

    let label = match (&hit.admin1, &hit.country) {
        (Some(a), Some(c)) if a != &hit.name => format!("{}, {}, {}", hit.name, a, c),
        (_, Some(c)) => format!("{}, {}", hit.name, c),
        _ => hit.name.clone(),
    };

    Ok(ResolvedPlace {
        label,
        latitude: hit.latitude,
        longitude: hit.longitude,
    })
}

async fn fetch_current(
    http: &reqwest::Client,
    lat: f64,
    lon: f64,
) -> Result<CurrentReading, String> {
    #[derive(Deserialize)]
    struct ForecastResp {
        current: CurrentBlock,
        daily: DailyBlock,
    }
    #[derive(Deserialize)]
    struct CurrentBlock {
        temperature_2m: f64,
        apparent_temperature: f64,
        weather_code: u32,
    }
    #[derive(Deserialize)]
    struct DailyBlock {
        temperature_2m_max: Vec<f64>,
        temperature_2m_min: Vec<f64>,
    }

    let resp: ForecastResp = http
        .get(FORECAST_URL)
        .query(&[
            ("latitude", lat.to_string()),
            ("longitude", lon.to_string()),
            (
                "current",
                "temperature_2m,apparent_temperature,weather_code".to_string(),
            ),
            ("daily", "temperature_2m_max,temperature_2m_min".to_string()),
            ("timezone", "auto".to_string()),
            ("forecast_days", "1".to_string()),
        ])
        .send()
        .await
        .map_err(|e| format!("forecast request failed: {}", e))?
        .error_for_status()
        .map_err(|e| format!("forecast returned error: {}", e))?
        .json()
        .await
        .map_err(|e| format!("could not parse forecast response: {}", e))?;

    let daily_high = *resp
        .daily
        .temperature_2m_max
        .first()
        .ok_or("forecast missing daily high")?;
    let daily_low = *resp
        .daily
        .temperature_2m_min
        .first()
        .ok_or("forecast missing daily low")?;

    Ok(CurrentReading {
        temperature: resp.current.temperature_2m,
        apparent_temperature: resp.current.apparent_temperature,
        weather_code: resp.current.weather_code,
        daily_high,
        daily_low,
    })
}

/// WMO weather interpretation code → short English description.
/// Reference: https://open-meteo.com/en/docs (WMO Weather interpretation codes).
fn describe(code: u32) -> &'static str {
    match code {
        0 => "Clear sky",
        1 => "Mainly clear",
        2 => "Partly cloudy",
        3 => "Overcast",
        45 | 48 => "Fog",
        51 | 53 | 55 => "Drizzle",
        56 | 57 => "Freezing drizzle",
        61 | 63 | 65 => "Rain",
        66 | 67 => "Freezing rain",
        71 | 73 | 75 => "Snow",
        77 => "Snow grains",
        80 | 81 | 82 => "Rain showers",
        85 | 86 => "Snow showers",
        95 => "Thunderstorm",
        96 | 99 => "Thunderstorm with hail",
        _ => "Unknown conditions",
    }
}
