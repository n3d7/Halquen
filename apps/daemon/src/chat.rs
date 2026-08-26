use halquen_domain::EntityId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalIntent {
    OpenApp {
        display_name: String,
        entity_id: EntityId,
    },
    RememberPreference {
        key: String,
        value: String,
    },
    ForgetPreference {
        key: String,
    },
}

pub fn resolve_local(message: &str) -> Option<LocalIntent> {
    let trimmed = message.trim();
    parse_open_app(trimmed)
        .or_else(|| parse_remember(trimmed))
        .or_else(|| parse_forget(trimmed))
}

pub fn normalize_request(message: &str) -> String {
    message
        .trim()
        .to_lowercase()
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || character.is_whitespace() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_open_app(message: &str) -> Option<LocalIntent> {
    let lower = message.to_lowercase();
    let raw_name = lower
        .strip_prefix("open ")
        .or_else(|| lower.strip_prefix("открой "))?;
    let display_name = clean_fragment(raw_name)?;
    let slug = display_name
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    let entity_id = EntityId::new(format!("app:{slug}")).ok()?;
    Some(LocalIntent::OpenApp {
        display_name,
        entity_id,
    })
}

fn parse_remember(message: &str) -> Option<LocalIntent> {
    let normalized = message.trim().trim_end_matches(['.', '!', '?']);
    let lower = normalized.to_lowercase();

    for prefix in [
        "remember that when i say ",
        "запомни, что когда я говорю ",
        "запомни что когда я говорю ",
    ] {
        if lower.starts_with(prefix) {
            let remainder = &normalized[prefix.len()..];
            let lower_remainder = remainder.to_lowercase();
            let separators = [" i mean ", " я имею в виду "];
            for separator in separators {
                if let Some(position) = lower_remainder.find(separator) {
                    let key = clean_fragment(&remainder[..position])?;
                    let value = clean_fragment(&remainder[position + separator.len()..])?;
                    return preference(key, value);
                }
            }
        }
    }

    for prefix in [
        "remember that my ",
        "запомни, что мой ",
        "запомни что мой ",
        "запомни, что моя ",
        "запомни что моя ",
    ] {
        if lower.starts_with(prefix) {
            let remainder = &normalized[prefix.len()..];
            let lower_remainder = remainder.to_lowercase();
            for separator in [" is ", " — ", " это "] {
                if let Some(position) = lower_remainder.find(separator) {
                    let key = clean_fragment(&remainder[..position])?;
                    let value = clean_fragment(&remainder[position + separator.len()..])?;
                    return preference(key, value);
                }
            }
        }
    }
    None
}

fn parse_forget(message: &str) -> Option<LocalIntent> {
    let lower = message.to_lowercase();
    let key = lower
        .strip_prefix("forget ")
        .or_else(|| lower.strip_prefix("забудь "))?;
    clean_fragment(key).map(|key| LocalIntent::ForgetPreference { key })
}

fn preference(key: String, value: String) -> Option<LocalIntent> {
    if key.len() > 256 || value.len() > 2_048 {
        return None;
    }
    Some(LocalIntent::RememberPreference { key, value })
}

fn clean_fragment(value: &str) -> Option<String> {
    let value = value
        .trim()
        .trim_matches(['"', '\'', '«', '»', '`'])
        .trim()
        .to_owned();
    (!value.is_empty() && value.len() <= 2_048).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_open_app_without_ai() {
        assert!(matches!(
            resolve_local("Open Telegram"),
            Some(LocalIntent::OpenApp { entity_id, .. }) if entity_id.as_str() == "app:telegram"
        ));
    }

    #[test]
    fn parses_explicit_alias_as_user_preference() {
        assert_eq!(
            resolve_local("Remember that when I say \"editor\" I mean Zed."),
            Some(LocalIntent::RememberPreference {
                key: "editor".to_owned(),
                value: "Zed".to_owned(),
            })
        );
    }

    #[test]
    fn normalization_is_exact_and_deterministic() {
        assert_eq!(normalize_request("  Hello,   HALQUEN! "), "hello halquen");
    }
}
