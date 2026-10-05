use crate::error::CoreError;
use serde_json::Value;

pub struct Environment {
    pub os: String,
    pub arch: String,
    pub version: String,
    pub demo: bool,
}
impl Environment {
    pub fn windows(demo: bool) -> Self {
        Self {
            os: "windows".into(),
            arch: "x86_64".into(),
            version: sysinfo::System::kernel_version().unwrap_or_default(),
            demo,
        }
    }
}

pub fn allowed(rules: Option<&Value>, env: &Environment) -> Result<bool, CoreError> {
    let Some(rules) = rules else {
        return Ok(true);
    };
    let rules = rules.as_array().ok_or(CoreError::UnsupportedVersion)?;
    let mut result = false;
    for rule in rules {
        let mut matches = true;
        if let Some(os) = rule.get("os") {
            if let Some(name) = os.get("name").and_then(Value::as_str) {
                matches &= name == env.os;
            }
            if let Some(arch) = os.get("arch").and_then(Value::as_str) {
                matches &= arch == env.arch || (arch == "amd64" && env.arch == "x86_64");
            }
            if let Some(version) = os.get("version").and_then(Value::as_str) {
                matches &= regex::Regex::new(version)
                    .map_err(|_| CoreError::UnsupportedVersion)?
                    .is_match(&env.version);
            }
            if let Some(range) = os.get("versionRange") {
                let numbers = |value: &str| -> Vec<u32> {
                    value
                        .split('.')
                        .map(|part| part.parse().unwrap_or(0))
                        .collect()
                };
                if let Some(min) = range.get("min").and_then(Value::as_str) {
                    matches &= numbers(&env.version) >= numbers(min);
                }
                if let Some(max) = range.get("max").and_then(Value::as_str) {
                    matches &= numbers(&env.version) < numbers(max);
                }
            }
        }
        if let Some(features) = rule.get("features") {
            for (key, value) in features.as_object().ok_or(CoreError::UnsupportedVersion)? {
                let actual = match key.as_str() {
                    "is_demo_user" => env.demo,
                    "has_custom_resolution" => true,
                    _ => false,
                };
                matches &= value.as_bool() == Some(actual);
            }
        }
        let allow = match rule.get("action").and_then(Value::as_str) {
            Some("allow") => true,
            Some("disallow") => false,
            _ => return Err(CoreError::UnsupportedVersion),
        };
        if matches {
            result = allow;
        }
    }
    Ok(result)
}

pub fn arguments(values: &Value, env: &Environment) -> Result<Vec<String>, CoreError> {
    let mut result = Vec::new();
    for value in values.as_array().ok_or(CoreError::UnsupportedVersion)? {
        if let Some(value) = value.as_str() {
            result.push(value.to_owned());
        } else if allowed(value.get("rules"), env)? {
            match value.get("value") {
                Some(Value::String(value)) => result.push(value.clone()),
                Some(Value::Array(values)) => {
                    for value in values {
                        result.push(
                            value
                                .as_str()
                                .ok_or(CoreError::UnsupportedVersion)?
                                .to_owned(),
                        );
                    }
                }
                _ => return Err(CoreError::UnsupportedVersion),
            }
        }
    }
    Ok(result)
}
