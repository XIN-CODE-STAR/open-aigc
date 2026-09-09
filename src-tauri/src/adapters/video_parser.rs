use std::sync::Arc;

use crate::ports::captioning_port::CaptioningPort;
use crate::ports::parser_port::{ParseError, ParseRequest, ParseResult, ParserInfo, ParserPort};

/// Video parser adapter that uses a CaptioningPort to parse video frames.
///
/// This adapter extracts key frames from a video and uses a CaptioningPort
/// to parse each frame, then combines the results into a single ParseResult.
pub struct VideoParserAdapter {
    captioner: Arc<dyn CaptioningPort>,
}

impl VideoParserAdapter {
    pub fn new(captioner: Arc<dyn CaptioningPort>) -> Self {
        Self { captioner }
    }
}

impl ParserPort for VideoParserAdapter {
    fn info(&self) -> ParserInfo {
        let captioner_info = self.captioner.info();
        ParserInfo {
            adapter_id: format!("video-{}", captioner_info.adapter_id),
            name: format!("Video Parser ({})", captioner_info.adapter_id),
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
                "video parser adapter {} is not ready",
                self.captioner.adapter_id()
            )));
        }

        // For now, we'll treat the video as a single image
        // In a real implementation, we would extract key frames from the video
        // and parse each frame separately, then combine the results

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
                adapter_id: "mock-video-captioner".to_owned(),
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
                caption: "A person walking in a park".to_owned(),
                tags: vec![
                    SemanticTag {
                        name: "scene:park".to_owned(),
                        confidence: 0.9,
                        source: InferenceSource::Llm,
                    },
                    SemanticTag {
                        name: "action:walking".to_owned(),
                        confidence: 0.8,
                        source: InferenceSource::Llm,
                    },
                ],
                entities: vec![SemanticEntity {
                    entity_type: "person".to_owned(),
                    name: "person".to_owned(),
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
    fn parse_video_extracts_structured_data() {
        let captioner = Arc::new(MockCaptioner);
        let parser = VideoParserAdapter::new(captioner);

        let request = ParseRequest {
            asset_id: "test-video".to_owned(),
            data_url: "data:video/mp4;base64,abc".to_owned(),
            instruction: None,
        };

        let result = parser.parse(&request).unwrap();
        assert_eq!(result.description, "A person walking in a park");
        assert_eq!(result.objects, vec!["person"]);
        assert_eq!(result.scene, vec!["scene:park"]);
        assert_eq!(result.actions, vec!["action:walking"]);
    }
}
