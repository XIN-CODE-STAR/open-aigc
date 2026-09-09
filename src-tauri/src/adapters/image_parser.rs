use std::sync::Arc;

use crate::ports::captioning_port::CaptioningPort;
use crate::ports::parser_port::{ParseError, ParseRequest, ParseResult, ParserInfo, ParserPort};

/// Image parser adapter that uses a CaptioningPort to parse images.
///
/// This adapter wraps a CaptioningPort and adapts its interface to the
/// ParserPort interface, adding support for extracting objects, scene,
/// actions, and concepts from images.
pub struct ImageParserAdapter {
    captioner: Arc<dyn CaptioningPort>,
}

impl ImageParserAdapter {
    pub fn new(captioner: Arc<dyn CaptioningPort>) -> Self {
        Self { captioner }
    }
}

impl ParserPort for ImageParserAdapter {
    fn info(&self) -> ParserInfo {
        let captioner_info = self.captioner.info();
        ParserInfo {
            adapter_id: format!("image-{}", captioner_info.adapter_id),
            name: format!("Image Parser ({})", captioner_info.adapter_id),
            ready: captioner_info.ready,
            supported_models: captioner_info.supported_models,
        }
    }

    fn adapter_id(&self) -> &str {
        // We can't return a dynamically created string, so we'll use the underlying adapter ID
        // In a real implementation, we might store this as a field
        self.captioner.adapter_id()
    }

    fn is_ready(&self) -> bool {
        self.captioner.is_ready()
    }

    fn parse(&self, request: &ParseRequest) -> Result<ParseResult, ParseError> {
        if !self.is_ready() {
            return Err(ParseError::NotReady(format!(
                "image parser adapter {} is not ready",
                self.captioner.adapter_id()
            )));
        }

        // Create a caption request
        let caption_request = crate::ports::captioning_port::CaptionRequest {
            image_url: request.data_url.clone(),
            instruction: request.instruction.clone(),
            asset_id: Some(request.asset_id.clone()),
        };

        // Call the captioner
        let caption_result = self
            .captioner
            .caption_image(&caption_request)
            .map_err(|e| ParseError::ParsingFailed(format!("captioning failed: {e}")))?;

        // Extract objects from entities
        let objects: Vec<String> = caption_result
            .entities
            .iter()
            .filter(|e| e.entity_type == "object" || e.entity_type == "person")
            .map(|e| e.name.clone())
            .collect();

        // Extract scene from tags
        let scene: Vec<String> = caption_result
            .tags
            .iter()
            .filter(|t| {
                t.name.starts_with("scene:")
                    || t.name.starts_with("location:")
                    || t.name.starts_with("environment:")
            })
            .map(|t| t.name.clone())
            .collect();

        // Extract actions from tags
        let actions: Vec<String> = caption_result
            .tags
            .iter()
            .filter(|t| t.name.starts_with("action:") || t.name.starts_with("activity:"))
            .map(|t| t.name.clone())
            .collect();

        // Extract concepts from tags
        let concepts: Vec<String> = caption_result
            .tags
            .iter()
            .filter(|t| {
                t.name.starts_with("concept:")
                    || t.name.starts_with("style:")
                    || t.name.starts_with("mood:")
            })
            .map(|t| t.name.clone())
            .collect();

        Ok(ParseResult {
            description: caption_result.caption,
            objects,
            scene,
            actions,
            concepts,
            ocr_text: caption_result.ocr_text,
            raw_response: caption_result.raw_response,
            tokens_used: caption_result.tokens_used,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::common::InferenceSource;
    use crate::domain::semantic::{SemanticEntity, SemanticTag};
    use crate::ports::captioning_port::{
        CaptionError, CaptionRequest, CaptionResult, CaptionerInfo, CaptioningPort,
    };

    struct MockCaptioner;

    impl CaptioningPort for MockCaptioner {
        fn info(&self) -> CaptionerInfo {
            CaptionerInfo {
                adapter_id: "mock-image-captioner".to_owned(),
                ready: true,
                supported_models: vec!["mock-model".to_owned()],
            }
        }

        fn adapter_id(&self) -> &str {
            "mock"
        }

        fn supported_models(&self) -> Vec<&str> {
            vec!["mock-model"]
        }

        fn is_ready(&self) -> bool {
            true
        }

        fn caption_image(&self, _request: &CaptionRequest) -> Result<CaptionResult, CaptionError> {
            Ok(CaptionResult {
                caption: "A cat sitting on a desk".to_owned(),
                tags: vec![
                    SemanticTag {
                        name: "scene:office".to_owned(),
                        confidence: 0.9,
                        source: InferenceSource::Llm,
                    },
                    SemanticTag {
                        name: "action:sitting".to_owned(),
                        confidence: 0.8,
                        source: InferenceSource::Llm,
                    },
                    SemanticTag {
                        name: "concept:cozy".to_owned(),
                        confidence: 0.7,
                        source: InferenceSource::Llm,
                    },
                ],
                entities: vec![SemanticEntity {
                    entity_type: "object".to_owned(),
                    name: "cat".to_owned(),
                    confidence: 0.95,
                    bbox: None,
                }],
                ocr_text: None,
                raw_response: "{}".to_owned(),
                tokens_used: Some(100),
            })
        }
    }

    #[test]
    fn parse_image_extracts_structured_data() {
        let captioner = Arc::new(MockCaptioner);
        let parser = ImageParserAdapter::new(captioner);

        let request = ParseRequest {
            asset_id: "test-asset".to_owned(),
            data_url: "data:image/png;base64,abc".to_owned(),
            instruction: None,
        };

        let result = parser.parse(&request).unwrap();
        assert_eq!(result.description, "A cat sitting on a desk");
        assert_eq!(result.objects, vec!["cat"]);
        assert_eq!(result.scene, vec!["scene:office"]);
        assert_eq!(result.actions, vec!["action:sitting"]);
        assert_eq!(result.concepts, vec!["concept:cozy"]);
    }
}
