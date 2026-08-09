use crate::{
    BUILTIN_RECOMMENDATION_CODES, BUILTIN_RULE_IDS, BUILTIN_RULE_SET_NAME,
    BUILTIN_RULE_SET_VERSION, CURRENT_ARTIFACT_SCHEMA_VERSION,
};

pub const AI_DIAGNOSIS_GUIDE_FILE_NAME: &str = "ai-diagnosis-guide.md";
pub const AI_DIAGNOSIS_GUIDE_MEDIA_TYPE: &str = "text/markdown; charset=utf-8";
pub const AI_DIAGNOSIS_GUIDE: &str = include_str!("ai-diagnosis-guide.md");

pub fn validate_ai_diagnosis_guide() -> Result<(), String> {
    for expected in [
        format!("artifact_schema_version: \"{CURRENT_ARTIFACT_SCHEMA_VERSION}\""),
        format!("  - name: {BUILTIN_RULE_SET_NAME}"),
        format!("    version: {BUILTIN_RULE_SET_VERSION}"),
    ] {
        if !AI_DIAGNOSIS_GUIDE.contains(&expected) {
            return Err(format!(
                "AI diagnosis guide is missing metadata {expected:?}"
            ));
        }
    }
    for rule_id in BUILTIN_RULE_IDS {
        if !AI_DIAGNOSIS_GUIDE.contains(&format!("`{rule_id}`")) {
            return Err(format!("AI diagnosis guide is missing rule_id {rule_id:?}"));
        }
    }
    for code in BUILTIN_RECOMMENDATION_CODES {
        if !AI_DIAGNOSIS_GUIDE.contains(&format!("`{code}`")) {
            return Err(format!(
                "AI diagnosis guide is missing recommendation code {code:?}"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guide_matches_builtin_rules_and_artifact_schema() {
        validate_ai_diagnosis_guide().unwrap();
    }

    #[test]
    fn guide_starts_with_machine_readable_metadata() {
        assert!(AI_DIAGNOSIS_GUIDE.starts_with("---\ndocument_type: pcdiag_ai_diagnosis_guide\n"));
        assert!(AI_DIAGNOSIS_GUIDE.contains("\nguide_version: 1.0.0\n"));
    }
}
