use std::sync::Arc;

use crate::domain::semantic::{
    ArtifactSemanticProfile, SemanticEntity, SemanticRelation, SemanticTag,
};
use crate::ports::parser_port::ParseResult;

/// Semantic normalizer that unifies different model outputs into a canonical schema.
///
/// This normalizer takes raw parser output and converts it into a standardized
/// ArtifactSemanticProfile, ensuring consistency across different parsers and models.
pub struct SemanticNormalizer;

impl SemanticNormalizer {
    /// Normalize a parser result into an ArtifactSemanticProfile.
    pub fn normalize(
        asset_id: &str,
        parser_id: &str,
        parse_result: &ParseResult,
    ) -> ArtifactSemanticProfile {
        // Convert objects to entities
        let entities: Vec<SemanticEntity> = parse_result
            .objects
            .iter()
            .map(|obj| SemanticEntity {
                entity_type: "object".to_owned(),
                name: obj.clone(),
                confidence: 0.9, // Default confidence
                bbox: None,
            })
            .collect();

        // Convert scene, actions, concepts to tags
        let mut tags = Vec::new();

        // Add scene tags
        for scene in &parse_result.scene {
            tags.push(SemanticTag {
                name: scene.clone(),
                confidence: 0.8,
                source: crate::domain::common::InferenceSource::Llm,
            });
        }

        // Add action tags
        for action in &parse_result.actions {
            tags.push(SemanticTag {
                name: action.clone(),
                confidence: 0.8,
                source: crate::domain::common::InferenceSource::Llm,
            });
        }

        // Add concept tags
        for concept in &parse_result.concepts {
            tags.push(SemanticTag {
                name: concept.clone(),
                confidence: 0.7,
                source: crate::domain::common::InferenceSource::Llm,
            });
        }

        // Extract description_short and description_detailed from the description
        let (description_short, description_detailed) =
            Self::split_description(&parse_result.description);

        ArtifactSemanticProfile {
            artifact_id: asset_id.to_owned(),
            caption: Some(parse_result.description.clone()),
            ocr_text: parse_result.ocr_text.clone(),
            tags,
            entities,
            embedding_id: None, // Will be set by embedding service
            analyzer: parser_id.to_owned(),
            analyzer_version: "1.0".to_owned(),
            analyzed_at: time::OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned()),
            // New fields
            description_short,
            description_detailed,
            objects: parse_result.objects.clone(),
            scene: parse_result.scene.clone(),
            actions: parse_result.actions.clone(),
            concepts: parse_result.concepts.clone(),
            relations: Vec::new(), // Relations are discovered separately
            analysis_job_id: None,
        }
    }

    /// Split a description into short and detailed versions.
    fn split_description(description: &str) -> (Option<String>, Option<String>) {
        if description.is_empty() {
            return (None, None);
        }

        // Short description: first sentence or first 100 characters
        let first_sentence = description.split('.').next().unwrap_or(description);
        let trimmed = first_sentence.trim();
        let short = if trimmed.chars().count() <= 100 {
            trimmed.to_owned()
        } else {
            // 按字符截断，避免多字节字符在字节边界被切开导致 panic。
            let truncated: String = trimmed.chars().take(97).collect();
            format!("{truncated}...")
        };

        // Detailed description: 与 short 相同则不重复存储。
        let detailed = if description != short {
            Some(description.to_owned())
        } else {
            None
        };

        (Some(short), detailed)
    }

    /// Merge multiple parser results into a single profile.
    pub fn merge_results(
        asset_id: &str,
        results: &[(String, ParseResult)], // (parser_id, result)
    ) -> ArtifactSemanticProfile {
        if results.is_empty() {
            return ArtifactSemanticProfile {
                artifact_id: asset_id.to_owned(),
                caption: None,
                ocr_text: None,
                tags: Vec::new(),
                entities: Vec::new(),
                embedding_id: None,
                analyzer: "none".to_owned(),
                analyzer_version: "1.0".to_owned(),
                analyzed_at: time::OffsetDateTime::now_utc()
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned()),
                description_short: None,
                description_detailed: None,
                objects: Vec::new(),
                scene: Vec::new(),
                actions: Vec::new(),
                concepts: Vec::new(),
                relations: Vec::new(),
                analysis_job_id: None,
            };
        }

        // Use the first result as the base
        let (first_parser_id, first_result) = &results[0];
        let mut profile = Self::normalize(asset_id, first_parser_id, first_result);

        // Merge additional results
        for (parser_id, result) in results.iter().skip(1) {
            // Merge objects (avoid duplicates)
            for obj in &result.objects {
                if !profile.objects.contains(obj) {
                    profile.objects.push(obj.clone());
                }
            }

            // Merge scene (avoid duplicates)
            for scene in &result.scene {
                if !profile.scene.contains(scene) {
                    profile.scene.push(scene.clone());
                }
            }

            // Merge actions (avoid duplicates)
            for action in &result.actions {
                if !profile.actions.contains(action) {
                    profile.actions.push(action.clone());
                }
            }

            // Merge concepts (avoid duplicates)
            for concept in &result.concepts {
                if !profile.concepts.contains(concept) {
                    profile.concepts.push(concept.clone());
                }
            }

            // Update analyzer to indicate multiple sources
            if !profile.analyzer.contains(parser_id) {
                profile.analyzer = format!("{},{}", profile.analyzer, parser_id);
            }
        }

        profile
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::common::InferenceSource;

    #[test]
    fn normalize_creates_correct_profile() {
        let parse_result = ParseResult {
            description: "A cat sitting on a desk in an office. The cat is sleeping.".to_owned(),
            objects: vec!["cat".to_owned(), "desk".to_owned()],
            scene: vec!["scene:office".to_owned()],
            actions: vec!["action:sitting".to_owned(), "action:sleeping".to_owned()],
            concepts: vec!["concept:cozy".to_owned()],
            ocr_text: Some("Hello World".to_owned()),
            raw_response: "{}".to_owned(),
            tokens_used: Some(100),
        };

        let profile = SemanticNormalizer::normalize("asset-1", "parser-1", &parse_result);

        assert_eq!(profile.artifact_id, "asset-1");
        assert_eq!(
            profile.caption,
            Some("A cat sitting on a desk in an office. The cat is sleeping.".to_owned())
        );
        assert_eq!(profile.ocr_text, Some("Hello World".to_owned()));
        assert_eq!(profile.analyzer, "parser-1");
        assert_eq!(profile.objects, vec!["cat", "desk"]);
        assert_eq!(profile.scene, vec!["scene:office"]);
        assert_eq!(profile.actions, vec!["action:sitting", "action:sleeping"]);
        assert_eq!(profile.concepts, vec!["concept:cozy"]);
        assert_eq!(profile.tags.len(), 4); // scene + actions + concept
        assert_eq!(profile.entities.len(), 2); // objects
    }

    #[test]
    fn split_description_creates_short_version() {
        let description =
            "A cat sitting on a desk in an office. The cat is sleeping peacefully.".to_owned();
        let (short, detailed) = SemanticNormalizer::split_description(&description);

        assert_eq!(
            short,
            Some("A cat sitting on a desk in an office".to_owned())
        );
        assert_eq!(detailed, Some(description));
    }

    #[test]
    fn merge_results_combines_multiple_sources() {
        let result1 = ParseResult {
            description: "A cat".to_owned(),
            objects: vec!["cat".to_owned()],
            scene: vec!["scene:office".to_owned()],
            actions: vec![],
            concepts: vec![],
            ocr_text: None,
            raw_response: "{}".to_owned(),
            tokens_used: None,
        };

        let result2 = ParseResult {
            description: "A desk".to_owned(),
            objects: vec!["desk".to_owned()],
            scene: vec![],
            actions: vec!["action:sitting".to_owned()],
            concepts: vec!["concept:cozy".to_owned()],
            ocr_text: None,
            raw_response: "{}".to_owned(),
            tokens_used: None,
        };

        let results = vec![
            ("parser-1".to_owned(), result1),
            ("parser-2".to_owned(), result2),
        ];

        let profile = SemanticNormalizer::merge_results("asset-1", &results);

        assert_eq!(profile.objects.len(), 2);
        assert!(profile.objects.contains(&"cat".to_owned()));
        assert!(profile.objects.contains(&"desk".to_owned()));
        assert_eq!(profile.scene.len(), 1);
        assert_eq!(profile.actions.len(), 1);
        assert_eq!(profile.concepts.len(), 1);
        assert!(profile.analyzer.contains("parser-1"));
        assert!(profile.analyzer.contains("parser-2"));
    }
}
