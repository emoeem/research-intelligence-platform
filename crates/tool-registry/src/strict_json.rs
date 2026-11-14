//! strict JSON 解析：重复键拒绝（fail-closed）。
//!
//! 动机与边界（收尾批修复，原 HANDOVER §七遗留项）：serde_json 默认
//! `Value` 反序列化对重复键取 last-wins 覆盖语义，与 YAML 通道
//! （serde_yaml_ng 对重复键直接报错）不对称；工具 Manifest 的
//! network 块（egress_allowlist 配置）与插件 Manifest 都走 JSON 通道，
//! 静默键覆盖在配置面是隐患。本模块用手工 `Deserialize` 实现
//! （serde 数据模型递归 + 每层 map 键集合查重），零新增依赖，
//! 语义与 YAML 通道对齐：任何层级的重复键即解析错误。
//!
//! 非字符串键：serde_json 的 `Value` 只承载字符串键，数字键经
//! `next_key::<String>` 走 serde 的键反序列化（JSON 规范本就只允许
//! 字符串键），与既有行为一致，不在此扩展。

use serde::de::{Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::Value;
use std::collections::HashSet;

/// strict 解析入口：与 `serde_json::from_str::<Value>` 同签名语义，
/// 差异仅在重复键处理（拒绝而非覆盖）。
pub fn from_str(doc: &str) -> Result<Value, serde_json::Error> {
    serde_json::from_str::<StrictValue>(doc).map(|StrictValue(v)| v)
}

/// 承载 strict 反序列化语义的 `Value` 包装。
pub struct StrictValue(Value);

impl<'de> serde::Deserialize<'de> for StrictValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct V;

        impl<'de> Visitor<'de> for V {
            type Value = StrictValue;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("any JSON value")
            }

            fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
            where
                E: serde::de::Error, {
                Ok(StrictValue(Value::Bool(v)))
            }

            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
            where
                E: serde::de::Error, {
                Ok(StrictValue(Value::Number(v.into())))
            }

            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: serde::de::Error, {
                Ok(StrictValue(Value::Number(v.into())))
            }

            fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
            where
                E: serde::de::Error, {
                let n = serde_json::Number::from_f64(v)
                    .ok_or_else(|| serde::de::Error::custom("non-finite float"))?;
                Ok(StrictValue(Value::Number(n)))
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error, {
                Ok(StrictValue(Value::String(v.to_owned())))
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E>
            where
                E: serde::de::Error, {
                Ok(StrictValue(Value::Null))
            }

            fn visit_none<E>(self) -> Result<Self::Value, E>
            where
                E: serde::de::Error, {
                Ok(StrictValue(Value::Null))
            }

            fn visit_some<D2>(self, d: D2) -> Result<Self::Value, D2::Error>
            where
                D2: Deserializer<'de>,
            {
                d.deserialize_any(V)
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut out = Vec::new();
                while let Some(item) = seq.next_element::<StrictValue>()? {
                    out.push(item.0);
                }
                Ok(StrictValue(Value::Array(out)))
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut seen: HashSet<String> = HashSet::new();
                let mut out = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if !seen.insert(key.clone()) {
                        return Err(serde::de::Error::custom(format!(
                            "duplicate key: {key}"
                        )));
                    }
                    let value = map.next_value::<StrictValue>()?;
                    out.insert(key, value.0);
                }
                Ok(StrictValue(Value::Object(out)))
            }
        }

        deserializer.deserialize_any(V)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_key_top_level_rejected() {
        let err = from_str(r#"{"a": 1, "b": 2, "a": 3}"#).unwrap_err();
        assert!(err.to_string().contains("duplicate key: a"), "{err}");
    }

    #[test]
    fn duplicate_key_nested_rejected() {
        let err = from_str(r#"{"net": {"mode": "none", "mode": "egress"}}"#).unwrap_err();
        assert!(err.to_string().contains("duplicate key: mode"), "{err}");
    }

    #[test]
    fn duplicate_key_in_array_rejected() {
        let err = from_str(r#"{"list": [{"k": 1}, {"k": 2, "k": 3}]}"#).unwrap_err();
        assert!(err.to_string().contains("duplicate key: k"), "{err}");
    }

    #[test]
    fn clean_document_parses_identically() {
        let doc = r#"{"s":"1","n":[1,2.5,null,true],"o":{"k":"v","arr":[]}}"#;
        let strict = from_str(doc).unwrap();
        let plain: Value = serde_json::from_str(doc).unwrap();
        assert_eq!(strict, plain);
    }

    #[test]
    fn syntax_error_surface_unchanged() {
        assert!(from_str("{not json").is_err());
        assert!(from_str("[]").is_ok());
    }
}
