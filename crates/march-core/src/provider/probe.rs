use super::{
    DeliveryPath, MessageContent, ProviderClient, ProviderResponse, RequestMessage,
    RequestOptions,
};
use crate::agent::TurnCancellation;
use crate::settings::ProviderType;
use anyhow::{Context, Result};
use serde_json::json;

impl ProviderClient {
    pub async fn suggest_task_title(&self, first_user_message: &str) -> Result<Option<String>> {
        let trimmed = first_user_message.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }

        let messages = vec![
            RequestMessage::system(
                "You generate concise task titles for a coding workspace.\n\
                 Return only the title text.\n\
                 Rules:\n\
                 - Prefer Simplified Chinese when the user writes Chinese.\n\
                 - Use 8-18 characters when possible.\n\
                 - Keep the concrete object, such as a file, module, or bug.\n\
                 - Remove filler like '帮我', '请你', '看一下', '继续'.\n\
                 - Do not use quotes, numbering, or trailing punctuation.",
            ),
            RequestMessage::user(format!("First user message:\n{}", trimmed)),
        ];
        let response = self
            .complete_simple_request(
                messages,
                RequestOptions {
                    model: self.config.model.clone(),
                    stream: false,
                    temperature: 0.1,
                    top_p: None,
                    presence_penalty: None,
                    frequency_penalty: None,
                    max_output_tokens: Some(64),
                },
            )
            .await
            .context("failed to request suggested title")?;

        Ok(response
            .content
            .as_deref()
            .and_then(super::title::sanitize_task_title)
            .or_else(|| super::fallback_task_title(trimmed)))
    }

    pub async fn test_connection(&self) -> Result<String> {
        let probe_model = self.resolve_probe_model_for_connection().await?;
        let reply = self.run_probe_request(&probe_model).await?;
        Ok(format!(
            "连接成功，已按 {} 通道完成最小消息往返，测试模型 {} 返回：{}",
            self.config.provider_type.as_db_value(),
            probe_model,
            reply
        ))
    }

    pub async fn probe_tool_use_support(&self, model: &str) -> Result<bool> {
        let mut provider = self.clone();
        provider.function_tools = vec![super::messages::FunctionToolDefinition {
            name: "ping".to_string(),
            description: "Call this tool exactly once to confirm tool support.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            }),
        }];

        let messages = vec![RequestMessage::user(
            "Call the ping tool exactly once. Do not answer with plain text.",
        )];
        let options = RequestOptions {
            model: model.to_string(),
            stream: false,
            temperature: 0.0,
            top_p: None,
            presence_penalty: None,
            frequency_penalty: None,
            max_output_tokens: Some(64),
        };
        let request_preview = super::wire::adapter_for(&provider.config).build_request(
            &provider.config,
            &messages,
            &provider.function_tools,
            &[],
            &options,
        )?;
        let request_json = serde_json::to_string_pretty(&request_preview.body)
            .context("failed to encode tool probe request")?;
        let response = provider
            .complete_non_streaming(
                &messages,
                &options,
                request_json,
                DeliveryPath::NonStreamingCached,
                TurnCancellation::never(),
            )
            .await
            .context("failed to run tool probe request")?;

        Ok(response.tool_calls.iter().any(|call| call.name == "ping"))
    }

    pub async fn probe_vision_support(&self, model: &str) -> Result<bool> {
        const PROBE_PNG_BASE64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a3X8AAAAASUVORK5CYII=";

        let response = self
            .complete_simple_request_with_model(
                model,
                vec![RequestMessage::user(MessageContent::from_parts(vec![
                    super::messages::MessageContentPart::Text(
                        "Describe this image in one short sentence.".to_string(),
                    ),
                    super::messages::MessageContentPart::Image {
                        media_type: "image/png".to_string(),
                        data_base64: PROBE_PNG_BASE64.to_string(),
                        name: Some("march-probe.png".to_string()),
                    },
                ]))],
                RequestOptions {
                    model: model.to_string(),
                    stream: false,
                    temperature: 0.0,
                    top_p: None,
                    presence_penalty: None,
                    frequency_penalty: None,
                    max_output_tokens: Some(64),
                },
            )
            .await
            .context("failed to run vision probe request")?;

        Ok(response
            .content
            .as_deref()
            .map(str::trim)
            .is_some_and(|text| !text.is_empty()))
    }

    async fn resolve_probe_model_for_connection(&self) -> Result<String> {
        let configured_model = self.config.model.trim();
        if !configured_model.is_empty() {
            return Ok(configured_model.to_string());
        }

        match self.config.provider_type {
            ProviderType::OpenAiCompat | ProviderType::Ollama => match self.list_models().await {
                Ok(models) => {
                    if let Some(model) = models.into_iter().find(|model| !model.trim().is_empty()) {
                        return Ok(model);
                    }
                    anyhow::bail!("provider 没有返回可用模型，无法完成真实对话测试")
                }
                Err(error) => Err(
                    error.context("failed to determine probe model for provider connection test")
                ),
            },
            _ => anyhow::bail!("provider probe model is empty"),
        }
    }

    async fn run_probe_request(&self, model: &str) -> Result<String> {
        let response = self
            .complete_simple_request_with_model(
                model,
                vec![RequestMessage::user(
                    "Return exactly `MARCH_OK` and nothing else. Do not call tools.",
                )],
                RequestOptions {
                    model: model.to_string(),
                    stream: false,
                    temperature: 0.0,
                    top_p: None,
                    presence_penalty: None,
                    frequency_penalty: None,
                    max_output_tokens: Some(16),
                },
            )
            .await
            .context("failed to run provider probe request")?;
        let reply = response
            .content
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .ok_or_else(|| anyhow::anyhow!("provider probe response did not contain text"))?;

        Ok(super::title::summarize_probe_reply(reply))
    }

    async fn complete_simple_request(
        &self,
        messages: Vec<RequestMessage>,
        options: RequestOptions,
    ) -> Result<ProviderResponse> {
        let model = options.model.clone();
        self.complete_simple_request_with_model(&model, messages, options)
            .await
    }

    async fn complete_simple_request_with_model(
        &self,
        model: &str,
        messages: Vec<RequestMessage>,
        mut options: RequestOptions,
    ) -> Result<ProviderResponse> {
        let mut provider = self.clone();
        provider.function_tools = Vec::new();
        options.model = model.to_string();
        let request_preview = super::wire::adapter_for(&provider.config).build_request(
            &provider.config,
            &messages,
            &provider.function_tools,
            &[],
            &options,
        )?;
        let request_json = serde_json::to_string_pretty(&request_preview.body)
            .context("failed to encode provider request")?;
        provider
            .complete_non_streaming(
                &messages,
                &options,
                request_json,
                DeliveryPath::NonStreamingCached,
                TurnCancellation::never(),
            )
            .await
    }
}
