use regex::Regex;
use std::collections::HashMap;
use super::category::SmsCategory;
use crate::config::ClassifierConfig;

#[derive(Clone)]
pub struct SmsClassifier {
    rules: HashMap<SmsCategory, (Vec<String>, Option<Vec<Regex>>)>,
}

impl SmsClassifier {
    pub fn new(config: ClassifierConfig) -> Self {
        let mut rules = HashMap::new();

        for (category_name, rule) in config.rules {
            let category = SmsCategory::from(category_name.as_str());
            let compiled_patterns = rule.patterns.map(|patterns| {
                patterns
                    .into_iter()
                    .filter_map(|p| Regex::new(&p).ok())
                    .collect()
            });
            rules.insert(category, (rule.keywords, compiled_patterns));
        }

        SmsClassifier { rules }
    }

    pub fn classify(&self, content: &str) -> SmsCategory {
        let content_lower = content.to_lowercase();

        let priority_order = [
            SmsCategory::Verification,
            SmsCategory::Finance,
            SmsCategory::Notification,
            SmsCategory::Promotion,
        ];

        for category in priority_order {
            if let Some((keywords, patterns)) = self.rules.get(&category) {
                for keyword in keywords {
                    if content_lower.contains(&keyword.to_lowercase()) {
                        return category;
                    }
                }

                if let Some(ref compiled_patterns) = patterns {
                    for pattern in compiled_patterns {
                        if pattern.is_match(&content_lower) {
                            return category;
                        }
                    }
                }
            }
        }

        SmsCategory::Default
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_classifier() -> SmsClassifier {
        let mut rules = HashMap::new();
        rules.insert(
            SmsCategory::Verification,
            (
                vec!["验证码".to_string(), "code".to_string()],
                Some(vec!["\\d{4,6}".to_string()]),
            ),
        );
        rules.insert(
            SmsCategory::Promotion,
            (vec!["优惠".to_string(), "促销".to_string()], None),
        );

        let config = ClassifierConfig {
            enabled: true,
            rules: HashMap::new(),
        };

        SmsClassifier::new(config)
    }

    #[test]
    fn test_classify_verification() {
        let config = ClassifierConfig {
            enabled: true,
            rules: {
                let mut m = HashMap::new();
                m.insert(
                    "验证码".to_string(),
                    crate::config::CategoryRule {
                        keywords: vec!["验证码".to_string()],
                        patterns: Some(vec!["\\d{4,6}".to_string()]),
                    },
                );
                m
            },
        };
        let classifier = SmsClassifier::new(config);
        assert_eq!(
            classifier.classify("您的验证码是123456"),
            SmsCategory::Verification
        );
    }
}