use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use ptc_domain::{InputDefinition, InputKind};
use serde_json::Value;

use crate::EngineError;

pub struct VariableResolver;

#[derive(Debug, Clone)]
pub struct ResolvedInputs {
    values: BTreeMap<String, Value>,
    secret_names: BTreeSet<String>,
}

impl VariableResolver {
    pub fn resolve(
        definitions: &[InputDefinition],
        mut supplied: BTreeMap<String, Value>,
    ) -> Result<ResolvedInputs, EngineError> {
        let known: BTreeSet<_> = definitions
            .iter()
            .map(|input| input.name.as_str())
            .collect();
        if let Some(unknown) = supplied.keys().find(|name| !known.contains(name.as_str())) {
            return Err(EngineError::InvalidInput {
                name: unknown.clone(),
                reason: "input is not declared by this workflow".to_owned(),
            });
        }

        let mut values = BTreeMap::new();
        let mut secret_names = BTreeSet::new();
        for definition in definitions {
            if definition.kind == InputKind::Secret {
                secret_names.insert(definition.name.clone());
            }
            let value = supplied
                .remove(&definition.name)
                .or_else(|| definition.default.clone());

            let Some(value) = value else {
                if definition.required {
                    return Err(EngineError::InvalidInput {
                        name: definition.name.clone(),
                        reason: "required value is missing".to_owned(),
                    });
                }
                values.insert(definition.name.clone(), Value::Null);
                continue;
            };
            validate_input_value(definition, &value, true)?;
            values.insert(definition.name.clone(), value);
        }

        Ok(ResolvedInputs {
            values,
            secret_names,
        })
    }
}

impl ResolvedInputs {
    pub fn render(&self, template: &str) -> Result<String, EngineError> {
        self.render_with_secrets(template, false)
    }

    pub fn render_redacted(&self, template: &str) -> Result<String, EngineError> {
        self.render_with_secrets(template, true)
    }

    fn render_with_secrets(
        &self,
        template: &str,
        redact_secrets: bool,
    ) -> Result<String, EngineError> {
        let mut output = String::new();
        let mut remaining = template;
        while let Some(start) = remaining.find('<') {
            output.push_str(&remaining[..start]);
            let after_start = &remaining[start + 1..];
            let Some(end) = after_start.find('>') else {
                output.push_str(&remaining[start..]);
                return Ok(output);
            };
            let name = &after_start[..end];
            if is_placeholder(name) {
                let value = self
                    .values
                    .get(name)
                    .ok_or_else(|| EngineError::UnresolvedVariable(name.to_owned()))?;
                if redact_secrets && self.secret_names.contains(name) {
                    output.push_str("***REDACTED***");
                } else {
                    match value {
                        Value::String(value) => output.push_str(value),
                        Value::Null => {}
                        value => output.push_str(&value.to_string()),
                    }
                }
            } else {
                output.push_str(&remaining[start..start + end + 2]);
            }
            remaining = &after_start[end + 1..];
        }
        output.push_str(remaining);
        Ok(output)
    }

    pub fn redact_output(&self, text: &str) -> String {
        self.secret_names
            .iter()
            .fold(text.to_owned(), |current, name| {
                let Some(secret) = self.values.get(name).and_then(Value::as_str) else {
                    return current;
                };
                if secret.is_empty() {
                    return current;
                }
                current.replace(secret, "***REDACTED***")
            })
    }

    pub fn redact_json(&self, value: &mut Value) {
        match value {
            Value::String(text) => *text = self.redact_output(text),
            Value::Array(items) => {
                for item in items {
                    self.redact_json(item);
                }
            }
            Value::Object(fields) => {
                for item in fields.values_mut() {
                    self.redact_json(item);
                }
            }
            _ => {}
        }
    }

    pub fn redacted_values(&self) -> BTreeMap<String, Value> {
        self.values
            .iter()
            .map(|(name, value)| {
                let stored = if self.secret_names.contains(name) {
                    Value::String("***REDACTED***".to_owned())
                } else {
                    value.clone()
                };
                (name.clone(), stored)
            })
            .collect()
    }
}

fn is_placeholder(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

pub(crate) fn validate_input_value(
    definition: &InputDefinition,
    value: &Value,
    check_file_exists: bool,
) -> Result<(), EngineError> {
    let string_is_present = |value: &str| !definition.required || !value.trim().is_empty();
    let valid = match definition.kind {
        InputKind::Text | InputKind::Textarea | InputKind::Secret => {
            value.as_str().is_some_and(string_is_present)
        }
        InputKind::File => value.as_str().is_some_and(|value| {
            string_is_present(value) && (!check_file_exists || Path::new(value).is_file())
        }),
        InputKind::Url => value.as_str().is_some_and(|value| {
            let Some((_, remainder)) = value.split_once("://") else {
                return false;
            };
            (value.starts_with("http://") || value.starts_with("https://"))
                && !remainder.is_empty()
                && !remainder.starts_with('/')
                && !remainder.chars().any(char::is_whitespace)
        }),
        InputKind::Number => value.is_number(),
        InputKind::Boolean => value.is_boolean(),
    };

    if valid {
        Ok(())
    } else {
        Err(EngineError::InvalidInput {
            name: definition.name.clone(),
            reason: match definition.kind {
                InputKind::File if check_file_exists => {
                    "expected a path to an existing file".to_owned()
                }
                InputKind::Url => "expected a valid http:// or https:// URL".to_owned(),
                _ => format!("value does not match {:?}", definition.kind),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(name: &str, kind: InputKind, required: bool) -> InputDefinition {
        InputDefinition {
            name: name.to_owned(),
            label: name.to_owned(),
            kind,
            required,
            description: None,
            default: None,
        }
    }

    #[test]
    fn renders_and_redacts_secrets() {
        let definitions = vec![
            input("TARGET", InputKind::Url, true),
            input("TOKEN", InputKind::Secret, true),
        ];
        let supplied = BTreeMap::from([
            (
                "TARGET".to_owned(),
                Value::String("https://example.test".to_owned()),
            ),
            ("TOKEN".to_owned(), Value::String("secret".to_owned())),
        ]);

        let resolved = VariableResolver::resolve(&definitions, supplied).unwrap();
        assert_eq!(
            resolved.render("Review <TARGET> using <TOKEN>").unwrap(),
            "Review https://example.test using secret"
        );
        assert_eq!(
            resolved.redacted_values()["TOKEN"],
            Value::String("***REDACTED***".to_owned())
        );
        assert_eq!(
            resolved
                .render_redacted("Review <TARGET> using <TOKEN>")
                .unwrap(),
            "Review https://example.test using ***REDACTED***"
        );
    }

    #[test]
    fn rejects_empty_required_text_and_invalid_urls() {
        let empty = VariableResolver::resolve(
            &[input("NAME", InputKind::Text, true)],
            BTreeMap::from([("NAME".to_owned(), Value::String("  ".to_owned()))]),
        );
        assert!(empty.is_err());

        let url = VariableResolver::resolve(
            &[input("TARGET", InputKind::Url, true)],
            BTreeMap::from([(
                "TARGET".to_owned(),
                Value::String("https:///missing-host".to_owned()),
            )]),
        );
        assert!(url.is_err());
    }

    #[test]
    fn preserves_html_and_does_not_expand_supplied_tags() {
        let resolved = VariableResolver::resolve(
            &[
                input("HTML", InputKind::Textarea, true),
                input("TOKEN", InputKind::Secret, true),
            ],
            BTreeMap::from([
                (
                    "HTML".to_owned(),
                    Value::String("<div><TOKEN></div>".to_owned()),
                ),
                ("TOKEN".to_owned(), Value::String("private".to_owned())),
            ]),
        )
        .unwrap();
        assert_eq!(
            resolved.render("Evidence: <HTML>").unwrap(),
            "Evidence: <div><TOKEN></div>"
        );
        assert_eq!(
            resolved.render_redacted("<HTML> <TOKEN>").unwrap(),
            "<div><TOKEN></div> ***REDACTED***"
        );
        let mut output = serde_json::json!({"quote":"private", "nested":["private"]});
        resolved.redact_json(&mut output);
        assert_eq!(output["nested"][0], "***REDACTED***");
    }
}
