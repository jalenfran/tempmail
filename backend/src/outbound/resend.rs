use crate::error::{AppError, AppResult};

pub struct ResendClient {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl ResendClient {
    pub fn new(api_key: &str, base_url: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: api_key.to_string(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    pub async fn send(
        &self,
        from: &str,
        to: &[String],
        subject: &str,
        text: Option<&str>,
        html: Option<&str>,
        in_reply_to: Option<&str>,
    ) -> AppResult<String> {
        let mut headers = serde_json::Map::new();
        if let Some(irt) = in_reply_to {
            headers.insert(
                "In-Reply-To".into(),
                serde_json::Value::String(irt.to_string()),
            );
            headers.insert(
                "References".into(),
                serde_json::Value::String(irt.to_string()),
            );
        }

        let mut body = serde_json::json!({
            "from": from,
            "to": to,
            "subject": subject,
        });
        if let Some(t) = text {
            body["text"] = serde_json::Value::String(t.to_string());
        }
        if let Some(h) = html {
            body["html"] = serde_json::Value::String(h.to_string());
        }
        if !headers.is_empty() {
            body["headers"] = serde_json::Value::Object(headers);
        }

        let url = format!("{}/emails", self.base_url);
        let res = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?;

        if !res.status().is_success() {
            let status = res.status();
            let text = res.text().await.unwrap_or_default();
            return Err(AppError::Internal(anyhow::anyhow!(
                "resend error {status}: {text}"
            )));
        }

        let v: serde_json::Value = res.json().await?;
        let id = v
            .get("id")
            .and_then(|x| x.as_str())
            .ok_or_else(|| AppError::Internal(anyhow::anyhow!("no id in resend response")))?
            .to_string();
        Ok(id)
    }
}
