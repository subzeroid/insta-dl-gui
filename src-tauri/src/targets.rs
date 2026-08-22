use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Target {
    Profile { username: String },
    Post { code: String },
}

impl Target {
    pub fn parse(input: &str) -> Option<Self> {
        let s = input.trim();
        if s.is_empty() {
            return None;
        }

        // @username
        let bare = s.strip_prefix('@').unwrap_or(s);
        if !s.contains("instagram.com") && !s.contains('/') && is_username(bare) {
            return Some(Self::Profile {
                username: bare.to_lowercase(),
            });
        }
        if !s.contains("instagram.com") {
            return None;
        }

        // https://(www.)instagram.com/<path>?<query>#<fragment>
        let rest = s.split("instagram.com").nth(1)?;
        let rest = rest.trim_start_matches('/');
        let rest = match rest.find(['?', '#']) {
            Some(i) => &rest[..i],
            None => rest,
        };
        let segments: Vec<&str> = rest.split('/').filter(|p| !p.is_empty()).collect();

        match segments.as_slice() {
            ["stories", username, _story_id] => Some(Self::Profile {
                username: username.to_lowercase(),
            }),
            [kind, code] if matches!(*kind, "p" | "reel" | "reels" | "tv") => Some(Self::Post {
                code: (*code).to_string(),
            }),
            [username] => Some(Self::Profile {
                username: username.to_lowercase(),
            }),
            _ => None,
        }
    }
}

fn is_username(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 30
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
        && !s.starts_with('.')
        && !s.ends_with('.')
}

/// Extract the shortcode from a media payload (`code` field or derive from pk).
pub fn shortcode_of(media: &serde_json::Value) -> Option<String> {
    media.get("code").and_then(|v| v.as_str()).map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bare_username() {
        assert_eq!(
            Target::parse("instagram"),
            Some(Target::Profile {
                username: "instagram".into()
            })
        );
        assert_eq!(
            Target::parse("@NatGeo"),
            Some(Target::Profile {
                username: "natgeo".into()
            })
        );
    }

    #[test]
    fn parses_post_urls() {
        for form in [
            "https://www.instagram.com/p/DXZlTiKEpxw/",
            "https://instagram.com/p/DXZlTiKEpxw",
            "https://www.instagram.com/reel/DXZlTiKEpxw/?igsh=xyz",
            "https://www.instagram.com/reels/DXZlTiKEpxw/",
        ] {
            assert_eq!(
                Target::parse(form),
                Some(Target::Post {
                    code: "DXZlTiKEpxw".into()
                }),
                "{form}"
            );
        }
    }

    #[test]
    fn parses_profile_urls() {
        assert_eq!(
            Target::parse("https://www.instagram.com/natgeo/"),
            Some(Target::Profile {
                username: "natgeo".into()
            })
        );
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(Target::parse(""), None);
        assert_eq!(Target::parse("hello world"), None);
        assert_eq!(Target::parse("https://example.com/p/abc"), None);
        assert_eq!(Target::parse("-leading-dash"), None);
    }
}
