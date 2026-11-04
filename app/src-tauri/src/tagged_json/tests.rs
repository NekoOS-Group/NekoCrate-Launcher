use crate::{Request, resource_manager::Action};
use serde::Deserialize;
use serde_json::{Value, json};

// Tauri decodes command arguments from a borrowed JSON Value. Exercise that
// path as well as an owned Value and a JSON document (e.g. test fixtures).
fn decode_each(payload: Value, check: impl Fn(Request)) {
    check(<Request as Deserialize>::deserialize(&payload).unwrap());
    check(serde_json::from_str(&payload.to_string()).unwrap());
    check(serde_json::from_value(payload).unwrap());
}

#[test]
fn unit_commands_and_unknown_fields_keep_the_existing_wire_format() {
    decode_each(
        json!({"op":"snapshot", "ignored":{"nested":[1,null,"猫咪"]}}),
        |request| assert!(matches!(request, Request::Snapshot)),
    );
    decode_each(
        json!({"op":"resource_manager","action":{"kind":"cache"}}),
        |request| {
            assert!(matches!(
                request,
                Request::ResourceManager {
                    action: Action::Cache
                }
            ));
        },
    );
}

#[test]
fn required_optional_and_default_fields_keep_their_meanings() {
    decode_each(json!({"op":"create","name":"猫咪盒子"}), |request| {
        let Request::Create {
            name,
            icon,
            version,
            directory,
        } = request
        else {
            panic!("wrong operation")
        };
        assert_eq!(name, "猫咪盒子");
        assert_eq!(icon, None);
        assert_eq!(directory, None);
        assert_eq!(version, "");
    });
    for value in [
        json!({"op":"select_account"}),
        json!({"op":"select_account","id":null}),
    ] {
        decode_each(value, |request| {
            assert!(matches!(request, Request::SelectAccount { id: None }))
        });
    }
    decode_each(
        json!({"op":"install","id":"a","version":"1.20.1"}),
        |request| {
            let Request::Install {
                id,
                repair,
                version,
            } = request
            else {
                panic!("wrong operation")
            };
            assert_eq!(id, "a");
            assert!(!repair);
            assert_eq!(version.as_deref(), Some("1.20.1"));
        },
    );
    decode_each(
        json!({"op":"preview_resource_drop","id":"a","version":"1.20.1"}),
        |request| {
            let Request::PreviewResourceDrop {
                paths,
                sources,
                world,
                kind,
                ..
            } = request
            else {
                panic!("wrong operation")
            };
            assert!(paths.is_empty() && sources.is_empty());
            assert!(world.is_none() && kind.is_none());
        },
    );
}

#[test]
fn nested_tags_paths_and_full_width_integers_are_preserved() {
    let path = r"E:\游戏\逆转未来\.minecraft\皮肤.png";
    decode_each(
        json!({"op":"resource_manager","action":{"kind":"import_skin","path":path}}),
        |request| {
            let Request::ResourceManager {
                action: Action::ImportSkin { path: actual },
            } = request
            else {
                panic!("wrong operation")
            };
            assert_eq!(actual, std::path::Path::new(path));
        },
    );
    decode_each(
        json!({"op":"open_crash_report","id":"a","version":"v","time":u64::MAX}),
        |request| {
            assert!(matches!(
                request,
                Request::OpenCrashReport { time: u64::MAX, .. }
            ));
        },
    );
}

#[test]
fn malformed_commands_are_rejected_before_dispatch() {
    for payload in [
        Value::Null,
        json!([]),
        json!("snapshot"),
        json!({}),
        json!({"op":null}),
        json!({"op":2}),
        json!({"op":"unknown_command"}),
        json!({"snapshot":{}}),
        json!({"op":"create"}),
        json!({"op":"create","name":5}),
        json!({"op":"create","name":"a","icon":256}),
        json!({"op":"install","id":"a","repair":"false"}),
        json!({"op":"open_crash_report","id":"a","version":"v","time":-1}),
        json!({"op":"open_crash_report","id":"a","version":"v","time":1.5}),
        json!({"op":"resource_manager","action":{"kind":"unknown_action"}}),
        json!({"op":"resource_manager","action":{"kind":"install_java","major":-1}}),
        json!({"op":"resource_manager","action":{"kind":"import_skin","path":[]}}),
    ] {
        assert!(
            <Request as Deserialize>::deserialize(&payload).is_err(),
            "accepted {payload}"
        );
    }
    for (payload, expected) in [
        (json!({}), "missing field `op`"),
        (json!({"op":"create"}), "missing field `name`"),
        (
            json!({"op":"resource_manager","action":{}}),
            "missing field `kind`",
        ),
    ] {
        let error = serde_json::from_value::<Request>(payload)
            .err()
            .expect("invalid request");
        assert_eq!(error.to_string(), expected);
    }
}
