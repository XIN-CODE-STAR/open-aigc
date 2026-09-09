use std::sync::Arc;

use crate::application::error::AppError;
use crate::application::semantic::normalizer::SemanticNormalizer;
use crate::domain::semantic::ArtifactSemanticProfile;
use crate::ports::parser_port::{ParseRequest, ParserPort};
use crate::ports::semantic_repository::SemanticRepository;

/// Analysis pipeline that orchestrates the entire analysis process.
///
/// This pipeline coordinates parsing, normalization, and storage of
/// semantic profiles for resources.
pub struct AnalysisPipeline {
    parsers: Vec<Arc<dyn ParserPort>>,
    semantic_repo: Arc<dyn SemanticRepository>,
}

impl AnalysisPipeline {
    pub fn new(
        parsers: Vec<Arc<dyn ParserPort>>,
        semantic_repo: Arc<dyn SemanticRepository>,
    ) -> Self {
        Self {
            parsers,
            semantic_repo,
        }
    }

    /// Analyze a resource using the appropriate parser.
    pub fn analyze(
        &self,
        asset_id: &str,
        data_url: &str,
        instruction: Option<&str>,
    ) -> Result<AnalysisResult, AppError> {
        // Find the first ready parser
        let parser = self
            .parsers
            .iter()
            .find(|p| p.is_ready())
            .ok_or_else(|| AppError::Workflow("no ready parser available".to_owned()))?;

        let parser_id = parser.adapter_id().to_owned();

        // Create parse request
        let request = ParseRequest {
            asset_id: asset_id.to_owned(),
            data_url: data_url.to_owned(),
            instruction: instruction.map(|s| s.to_owned()),
        };

        // Parse the resource
        let parse_result = parser
            .parse(&request)
            .map_err(|e| AppError::Workflow(format!("parsing failed: {}", e)))?;

        // Normalize the result
        let profile = SemanticNormalizer::normalize(asset_id, &parser_id, &parse_result);

        // Store the profile
        self.semantic_repo.upsert_profile(&profile)?;

        Ok(AnalysisResult {
            profile,
            parser_id,
            parse_result,
        })
    }

    /// Analyze a resource using multiple parsers and merge results.
    pub fn analyze_with_multiple_parsers(
        &self,
        asset_id: &str,
        data_url: &str,
        instruction: Option<&str>,
    ) -> Result<AnalysisResult, AppError> {
        let mut results = Vec::new();

        // Collect results from all ready parsers
        for parser in &self.parsers {
            if !parser.is_ready() {
                continue;
            }

            let parser_id = parser.adapter_id().to_owned();

            let request = ParseRequest {
                asset_id: asset_id.to_owned(),
                data_url: data_url.to_owned(),
                instruction: instruction.map(|s| s.to_owned()),
            };

            match parser.parse(&request) {
                Ok(parse_result) => {
                    results.push((parser_id, parse_result));
                }
                Err(e) => {
                    eprintln!(
                        "[AnalysisPipeline] parser {} failed: {}",
                        parser.adapter_id(),
                        e
                    );
                    // Continue with other parsers
                }
            }
        }

        if results.is_empty() {
            return Err(AppError::Workflow("all parsers failed".to_owned()));
        }

        // Merge results from multiple parsers
        let profile = SemanticNormalizer::merge_results(asset_id, &results);

        // Store the merged profile
        self.semantic_repo.upsert_profile(&profile)?;

        // Use the first result for the response
        let (first_parser_id, first_parse_result) = results.into_iter().next().unwrap();

        Ok(AnalysisResult {
            profile,
            parser_id: first_parser_id,
            parse_result: first_parse_result,
        })
    }

    /// Get a previously analyzed profile.
    pub fn get_profile(&self, asset_id: &str) -> Result<Option<ArtifactSemanticProfile>, AppError> {
        self.semantic_repo.get_profile(asset_id)
    }

    /// Check if a resource has been analyzed.
    pub fn is_analyzed(&self, asset_id: &str) -> Result<bool, AppError> {
        let profile = self.semantic_repo.get_profile(asset_id)?;
        Ok(profile.is_some())
    }

    /// Get available parsers.
    pub fn available_parsers(&self) -> Vec<ParserInfo> {
        self.parsers
            .iter()
            .map(|p| {
                let info = p.info();
                ParserInfo {
                    adapter_id: info.adapter_id,
                    name: info.name,
                    ready: info.ready,
                }
            })
            .collect()
    }
}

/// Result of an analysis.
#[derive(Debug, Clone)]
pub struct AnalysisResult {
    /// The semantic profile.
    pub profile: ArtifactSemanticProfile,
    /// The parser ID used.
    pub parser_id: String,
    /// The raw parse result.
    pub parse_result: crate::ports::parser_port::ParseResult,
}

/// Information about a parser.
#[derive(Debug, Clone)]
pub struct ParserInfo {
    pub adapter_id: String,
    pub name: String,
    pub ready: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::ports::parser_port::{
        ParseError, ParseResult, ParserInfo as PortParserInfo, ParserPort,
    };

    struct MockParser {
        ready: bool,
    }

    impl ParserPort for MockParser {
        fn info(&self) -> PortParserInfo {
            PortParserInfo {
                adapter_id: "mock".to_owned(),
                name: "Mock Parser".to_owned(),
                ready: self.ready,
                supported_models: vec!["mock-model".to_owned()],
            }
        }

        fn adapter_id(&self) -> &str {
            "mock"
        }

        fn is_ready(&self) -> bool {
            self.ready
        }

        fn parse(&self, _request: &ParseRequest) -> Result<ParseResult, ParseError> {
            Ok(ParseResult {
                description: "A test image".to_owned(),
                objects: vec!["test".to_owned()],
                scene: vec!["scene:test".to_owned()],
                actions: vec![],
                concepts: vec![],
                ocr_text: None,
                raw_response: "{}".to_owned(),
                tokens_used: Some(100),
            })
        }
    }

    #[test]
    fn analyze_creates_profile() {
        use rusqlite::Connection;

        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE artifact_semantic_profiles (
                artifact_id TEXT PRIMARY KEY,
                caption TEXT,
                ocr_text TEXT,
                tags_json TEXT NOT NULL DEFAULT '[]',
                entities_json TEXT NOT NULL DEFAULT '[]',
                embedding_id TEXT,
                analyzer TEXT NOT NULL,
                analyzer_version TEXT NOT NULL,
                analyzed_at TEXT NOT NULL,
                description_short TEXT,
                description_detailed TEXT,
                objects_json TEXT NOT NULL DEFAULT '[]',
                scene_json TEXT NOT NULL DEFAULT '[]',
                actions_json TEXT NOT NULL DEFAULT '[]',
                concepts_json TEXT NOT NULL DEFAULT '[]',
                relations_json TEXT NOT NULL DEFAULT '[]',
                analysis_job_id TEXT
            );",
        )
        .unwrap();

        let repo = Arc::new(
            crate::adapters::sqlite::semantic_repository::SqliteSemanticRepository::open(conn),
        );
        let parser = Arc::new(MockParser { ready: true });
        let pipeline = AnalysisPipeline::new(vec![parser], repo);

        let result = pipeline
            .analyze("asset-1", "data:image/png;base64,abc", None)
            .unwrap();
        assert_eq!(result.profile.artifact_id, "asset-1");
        assert_eq!(result.parser_id, "mock");
    }
}
