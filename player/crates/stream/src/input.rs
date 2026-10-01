//! Data-channel message parsing and coordinate mapping.

use crate::InputEvent;
use serde::Deserialize;
use serde_json::Value;

/// One decoded data-channel message.
#[derive(Debug, PartialEq)]
pub(crate) enum Message {
    /// Zero or more events for the `input` callback, in order.
    Events(Vec<InputEvent>),
    /// A `ping`; the reply carries `c` back unchanged.
    Ping(Value),
}

#[derive(Deserialize)]
struct Wire {
    t: String,
    #[serde(default)]
    code: String,
    #[serde(default)]
    key: String,
    #[serde(default)]
    r: Value,
    #[serde(default)]
    b: u8,
    x: Option<f64>,
    y: Option<f64>,
    #[serde(default)]
    s: String,
    #[serde(default)]
    g: Value,
    #[serde(default)]
    c: Value,
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        _ => false,
    }
}

/// Maps normalized 0..1 coordinates of the video box to pixels of `size`.
/// Values outside 0..1 are clamped; the result is at most `size - 1`.
pub(crate) fn map_point(x: f64, y: f64, size: (u32, u32)) -> (f64, f64) {
    let m = |v: f64, n: u32| {
        let v = if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 };
        (v * f64::from(n)).min(f64::from(n.saturating_sub(1)))
    };
    (m(x, size.0), m(y, size.1))
}

/// Parses one text message. Returns `None` for malformed or unknown messages.
pub(crate) fn parse(text: &str, size: (u32, u32)) -> Option<Message> {
    let w: Wire = serde_json::from_str(text).ok()?;
    let pos = match (w.x, w.y) {
        (Some(x), Some(y)) => {
            let (x, y) = map_point(x, y, size);
            Some(InputEvent::MouseMove { x, y })
        }
        _ => None,
    };
    let mut ev = Vec::new();
    match w.t.as_str() {
        "kd" | "ku" => ev.push(InputEvent::Key {
            down: w.t == "kd",
            code: w.code,
            key: w.key,
            repeat: truthy(&w.r),
        }),
        "mm" => ev.extend(pos),
        "md" | "mu" => {
            ev.extend(pos);
            ev.push(InputEvent::MouseButton { down: w.t == "md", button: w.b });
        }
        "wh" => {
            ev.extend(pos);
            ev.push(InputEvent::Wheel { dx: w.x.unwrap_or(0.0), dy: w.y.unwrap_or(0.0) });
        }
        "tx" => ev.push(InputEvent::Text(w.s)),
        "fc" => ev.push(InputEvent::Focus(truthy(&w.g))),
        "ping" => return Some(Message::Ping(w.c)),
        _ => return None,
    }
    Some(Message::Events(ev))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SZ: (u32, u32) = (1280, 720);

    fn events(s: &str) -> Vec<InputEvent> {
        match parse(s, SZ) {
            Some(Message::Events(e)) => e,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn key_down_and_repeat() {
        assert_eq!(
            events(r#"{"t":"kd","code":"Enter","key":"Enter","r":1}"#),
            vec![InputEvent::Key { down: true, code: "Enter".into(), key: "Enter".into(), repeat: true }]
        );
        assert_eq!(
            events(r#"{"t":"ku","code":"KeyA","key":"a","r":0}"#),
            vec![InputEvent::Key { down: false, code: "KeyA".into(), key: "a".into(), repeat: false }]
        );
    }

    #[test]
    fn move_maps_to_pixels_and_clamps() {
        assert_eq!(events(r#"{"t":"mm","x":0.5,"y":0.25}"#), vec![InputEvent::MouseMove { x: 640.0, y: 180.0 }]);
        assert_eq!(events(r#"{"t":"mm","x":-3,"y":9}"#), vec![InputEvent::MouseMove { x: 0.0, y: 719.0 }]);
        assert_eq!(events(r#"{"t":"mm","x":1,"y":1}"#), vec![InputEvent::MouseMove { x: 1279.0, y: 719.0 }]);
    }

    #[test]
    fn button_moves_pointer_first() {
        assert_eq!(
            events(r#"{"t":"md","b":2,"x":0.25,"y":0.5}"#),
            vec![InputEvent::MouseMove { x: 320.0, y: 360.0 }, InputEvent::MouseButton { down: true, button: 2 }]
        );
        assert_eq!(events(r#"{"t":"mu","b":0}"#), vec![InputEvent::MouseButton { down: false, button: 0 }]);
    }

    #[test]
    fn wheel_is_not_normalized() {
        assert_eq!(events(r#"{"t":"wh","x":0,"y":-1}"#), vec![InputEvent::Wheel { dx: 0.0, dy: -1.0 }]);
    }

    #[test]
    fn text_and_focus() {
        assert_eq!(events(r#"{"t":"tx","s":"é"}"#), vec![InputEvent::Text("é".into())]);
        assert_eq!(events(r#"{"t":"fc","g":1}"#), vec![InputEvent::Focus(true)]);
        assert_eq!(events(r#"{"t":"fc","g":0}"#), vec![InputEvent::Focus(false)]);
    }

    #[test]
    fn ping_echoes_page_clock() {
        assert_eq!(parse(r#"{"t":"ping","c":1234.5}"#, SZ), Some(Message::Ping(serde_json::json!(1234.5))));
    }

    #[test]
    fn malformed_is_dropped() {
        assert_eq!(parse("not json", SZ), None);
        assert_eq!(parse(r#"{"t":"zz"}"#, SZ), None);
        assert_eq!(parse(r#"{"x":1}"#, SZ), None);
    }
}
