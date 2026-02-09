use serde_json::Value;
use shared_types::ViewerDescriptor;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewerWindowProps {
    pub descriptor: ViewerDescriptor,
}

pub fn parse_viewer_window_props(props: &Value) -> Result<ViewerWindowProps, String> {
    let viewer = props
        .get("viewer")
        .ok_or_else(|| "missing viewer descriptor".to_string())?;
    let descriptor: ViewerDescriptor =
        serde_json::from_value(viewer.clone()).map_err(|e| format!("invalid viewer: {e}"))?;
    Ok(ViewerWindowProps { descriptor })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_valid_viewer_descriptor() {
        let props = json!({
            "viewer": {
                "kind": "text",
                "resource": {
                    "uri": "sandbox://Cargo.toml",
                    "mime": "text/plain"
                },
                "capabilities": { "readonly": false }
            }
        });
        let parsed = parse_viewer_window_props(&props).expect("should parse");
        assert_eq!(parsed.descriptor.resource.mime, "text/plain");
    }

    #[test]
    fn missing_descriptor_errors() {
        let props = json!({});
        let err = parse_viewer_window_props(&props).expect_err("must fail");
        assert!(err.contains("missing viewer descriptor"));
    }
}
