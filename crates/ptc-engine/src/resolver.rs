use std::collections::{BTreeMap, BTreeSet};

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
            validate_type(definition, &value)?;
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
        let mut output = template.to_owned();
        for (name, value) in &self.values {
            let replacement = match value {
                Value::String(value) => value.clone(),
                Value::Null => String::new(),
                value => value.to_string(),
            };
            output = output.replace(&format!("<{name}>"), &replacement);
        }

        if let Some(start) = output.find('<') {
            if let Some(end) = output[start..].find('>') {
                return Err(EngineError::UnresolvedVariable(
                    output[start + 1..start + end].to_owned(),
                ));
            }
        }
        Ok(output)
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

fn validate_type(definition: &InputDefinition, value: &Value) -> Result<(), EngineError> {
    let valid = match definition.kind {
        InputKind::Text | InputKind::Textarea | InputKind::File | InputKind::Secret => {
            value.is_string()
        }
        InputKind::Url => value
            .as_str()
            .is_some_and(|value| value.starts_with("http://") || value.starts_with("https://")),
        InputKind::Number => value.is_number(),
        InputKind::Boolean => value.is_boolean(),
    };

    if valid {
        Ok(())
    } else {
        Err(EngineError::InvalidInput {
            name: definition.name.clone(),
            reason: format!("value does not match {:?}", definition.kind),
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
    }
}
