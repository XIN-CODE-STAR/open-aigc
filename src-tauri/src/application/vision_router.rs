use std::sync::Arc;

use crate::ports::parser_port::{ParseError, ParseRequest, ParseResult, ParserPort};
use crate::ports::vision_reasoning_port::{
    VisionReasoningError, VisionReasoningPort, VisionReasoningRequest, VisionReasoningResult,
};

/// Vision task complexity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisionTaskComplexity {
    /// Simple tasks: captioning, OCR, object detection
    Simple,
    /// Medium tasks: scene understanding, activity recognition
    Medium,
    /// Complex tasks: visual reasoning, question answering
    Complex,
}

/// Vision router that routes tasks to appropriate models based on complexity.
///
/// This router selects the appropriate parser or reasoner based on the
/// task complexity, optimizing for cost and performance.
pub struct VisionRouter {
    simple_parser: Option<Arc<dyn ParserPort>>,
    medium_parser: Option<Arc<dyn ParserPort>>,
    complex_reasoner: Option<Arc<dyn VisionReasoningPort>>,
}

impl VisionRouter {
    pub fn new(
        simple_parser: Option<Arc<dyn ParserPort>>,
        medium_parser: Option<Arc<dyn ParserPort>>,
        complex_reasoner: Option<Arc<dyn VisionReasoningPort>>,
    ) -> Self {
        Self {
            simple_parser,
            medium_parser,
            complex_reasoner,
        }
    }

    /// Determine the complexity of a vision task based on the request.
    pub fn determine_complexity(request: &ParseRequest) -> VisionTaskComplexity {
        // Simple heuristic: if there's an instruction, it's likely more complex
        if let Some(instruction) = &request.instruction {
            let instruction_lower = instruction.to_lowercase();

            // Complex tasks: reasoning, explanation, analysis
            if instruction_lower.contains("why")
                || instruction_lower.contains("how")
                || instruction_lower.contains("explain")
                || instruction_lower.contains("analyze")
                || instruction_lower.contains("compare")
                || instruction_lower.contains("relationship")
            {
                return VisionTaskComplexity::Complex;
            }

            // Medium tasks: description, scene understanding
            if instruction_lower.contains("describe")
                || instruction_lower.contains("what is")
                || instruction_lower.contains("what's")
                || instruction_lower.contains("scene")
                || instruction_lower.contains("context")
            {
                return VisionTaskComplexity::Medium;
            }
        }

        // Default to simple
        VisionTaskComplexity::Simple
    }

    /// Parse a resource using the appropriate parser based on complexity.
    pub fn parse(&self, request: &ParseRequest) -> Result<ParseResult, ParseError> {
        let complexity = Self::determine_complexity(request);

        match complexity {
            VisionTaskComplexity::Simple => {
                if let Some(parser) = &self.simple_parser {
                    parser.parse(request)
                } else if let Some(parser) = &self.medium_parser {
                    // Fall back to medium parser if simple is not available
                    parser.parse(request)
                } else {
                    Err(ParseError::NotReady(
                        "no simple parser available".to_owned(),
                    ))
                }
            }
            VisionTaskComplexity::Medium => {
                if let Some(parser) = &self.medium_parser {
                    parser.parse(request)
                } else if let Some(parser) = &self.simple_parser {
                    // Fall back to simple parser if medium is not available
                    parser.parse(request)
                } else {
                    Err(ParseError::NotReady(
                        "no medium parser available".to_owned(),
                    ))
                }
            }
            VisionTaskComplexity::Complex => {
                // For complex tasks, we should use the vision reasoner
                // But since ParserPort and VisionReasoningPort have different interfaces,
                // we'll use the medium parser as a fallback
                if let Some(parser) = &self.medium_parser {
                    parser.parse(request)
                } else if let Some(parser) = &self.simple_parser {
                    parser.parse(request)
                } else {
                    Err(ParseError::NotReady(
                        "no parser available for complex tasks".to_owned(),
                    ))
                }
            }
        }
    }

    /// Perform vision reasoning using the appropriate reasoner.
    pub fn reason(
        &self,
        request: &VisionReasoningRequest,
    ) -> Result<VisionReasoningResult, VisionReasoningError> {
        if let Some(reasoner) = &self.complex_reasoner {
            reasoner.reason(request)
        } else {
            Err(VisionReasoningError::NotReady(
                "no vision reasoner available".to_owned(),
            ))
        }
    }

    /// Get information about available vision adapters.
    pub fn available_adapters(&self) -> Vec<VisionAdapterInfo> {
        let mut adapters = Vec::new();

        if let Some(parser) = &self.simple_parser {
            let info = parser.info();
            adapters.push(VisionAdapterInfo {
                adapter_id: info.adapter_id,
                name: info.name,
                ready: info.ready,
                adapter_type: VisionAdapterType::Parser,
                complexity: VisionTaskComplexity::Simple,
            });
        }

        if let Some(parser) = &self.medium_parser {
            let info = parser.info();
            adapters.push(VisionAdapterInfo {
                adapter_id: info.adapter_id,
                name: info.name,
                ready: info.ready,
                adapter_type: VisionAdapterType::Parser,
                complexity: VisionTaskComplexity::Medium,
            });
        }

        if let Some(reasoner) = &self.complex_reasoner {
            let info = reasoner.info();
            adapters.push(VisionAdapterInfo {
                adapter_id: info.adapter_id,
                name: info.name,
                ready: info.ready,
                adapter_type: VisionAdapterType::Reasoner,
                complexity: VisionTaskComplexity::Complex,
            });
        }

        adapters
    }
}

/// Information about a vision adapter.
#[derive(Debug, Clone)]
pub struct VisionAdapterInfo {
    pub adapter_id: String,
    pub name: String,
    pub ready: bool,
    pub adapter_type: VisionAdapterType,
    pub complexity: VisionTaskComplexity,
}

/// Type of vision adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisionAdapterType {
    Parser,
    Reasoner,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::common::InferenceSource;
    use crate::domain::semantic::{SemanticEntity, SemanticTag};
    use crate::ports::captioning_port::{
        CaptionError, CaptionRequest, CaptionResult, CaptionerInfo, CaptioningPort,
    };

    struct MockCaptioner {
        ready: bool,
    }

    impl CaptioningPort for MockCaptioner {
        fn info(&self) -> CaptionerInfo {
            CaptionerInfo {
                adapter_id: "mock-captioner".to_owned(),
                ready: self.ready,
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
            self.ready
        }

        fn caption_image(&self, _request: &CaptionRequest) -> Result<CaptionResult, CaptionError> {
            Ok(CaptionResult {
                caption: "Test caption".to_owned(),
                tags: vec![SemanticTag {
                    name: "test".to_owned(),
                    confidence: 0.9,
                    source: InferenceSource::Llm,
                }],
                entities: vec![SemanticEntity {
                    entity_type: "object".to_owned(),
                    name: "test".to_owned(),
                    confidence: 0.95,
                    bbox: None,
                }],
                ocr_text: None,
                raw_response: "{}".to_owned(),
                tokens_used: Some(100),
            })
        }
    }

    struct MockParser {
        captioner: Arc<dyn CaptioningPort>,
    }

    impl ParserPort for MockParser {
        fn info(&self) -> crate::ports::parser_port::ParserInfo {
            let captioner_info = self.captioner.info();
            crate::ports::parser_port::ParserInfo {
                adapter_id: captioner_info.adapter_id.clone(),
                name: captioner_info.adapter_id,
                ready: captioner_info.ready,
                supported_models: captioner_info.supported_models,
            }
        }

        fn adapter_id(&self) -> &str {
            self.captioner.adapter_id()
        }

        fn is_ready(&self) -> bool {
            self.captioner.is_ready()
        }

        fn parse(&self, request: &ParseRequest) -> Result<ParseResult, ParseError> {
            let caption_request = CaptionRequest {
                image_url: request.data_url.clone(),
                instruction: request.instruction.clone(),
                asset_id: Some(request.asset_id.clone()),
            };

            let caption_result = self
                .captioner
                .caption_image(&caption_request)
                .map_err(|e| ParseError::ParsingFailed(format!("captioning failed: {e}")))?;

            Ok(ParseResult {
                description: caption_result.caption,
                objects: vec!["test".to_owned()],
                scene: vec![],
                actions: vec![],
                concepts: vec![],
                ocr_text: caption_result.ocr_text,
                raw_response: caption_result.raw_response,
                tokens_used: caption_result.tokens_used,
            })
        }
    }

    #[test]
    fn vision_router_selects_appropriate_parser() {
        let captioner = Arc::new(MockCaptioner { ready: true });
        let parser = Arc::new(MockParser { captioner });

        let router = VisionRouter::new(Some(parser.clone()), Some(parser.clone()), None);

        let simple_request = ParseRequest {
            asset_id: "test".to_owned(),
            data_url: "data:image/png;base64,abc".to_owned(),
            instruction: None,
        };

        let result = router.parse(&simple_request).unwrap();
        assert_eq!(result.description, "Test caption");
    }
}
