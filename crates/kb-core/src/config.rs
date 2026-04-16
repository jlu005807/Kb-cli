//! 通用配置合并模型。
//! 提供“多层来源 + 明确优先级 + 合并轨迹(trace)”能力，
//! 供 CLI 运行时配置接线复用。

use serde::{Deserialize, Serialize};
use std::fmt;

// CLI 运行时可合并配置模型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RuntimeConfigDto {
    pub quiet: bool,
    pub verbose: u8,
    pub no_color: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConfigSource {
    Flags,
    Env,
    Project,
    Global,
    Defaults,
}

impl ConfigSource {
    // 优先级从高到低：flags > env > project > global > defaults。
    pub const fn precedence_order() -> [Self; 5] {
        [
            Self::Flags,
            Self::Env,
            Self::Project,
            Self::Global,
            Self::Defaults,
        ]
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Flags => "flags",
            Self::Env => "env",
            Self::Project => "project",
            Self::Global => "global",
            Self::Defaults => "defaults",
        }
    }
}

impl fmt::Display for ConfigSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigMergeTrace {
    steps: Vec<String>,
}

impl ConfigMergeTrace {
    /// 原始合并步骤列表。
    pub fn steps(&self) -> &[String] {
        &self.steps
    }

    /// 以换行文本渲染，适合直接输出到 debug 日志。
    pub fn render(&self) -> String {
        self.steps.join("\n")
    }

    pub fn contains_fragment(&self, fragment: &str) -> bool {
        self.steps.iter().any(|step| step.contains(fragment))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeResult<T> {
    /// 最终选中的配置值。
    pub value: T,
    /// 最终值来源层。
    pub source: ConfigSource,
    /// 逐层判定轨迹。
    pub trace: ConfigMergeTrace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigLayers<T> {
    pub flags: Option<T>,
    pub env: Option<T>,
    pub project: Option<T>,
    pub global: Option<T>,
    pub defaults: T,
}

impl<T: Clone> ConfigLayers<T> {
    /// 使用仅 defaults 的最小配置层构造器。
    pub fn new(defaults: T) -> Self {
        Self {
            flags: None,
            env: None,
            project: None,
            global: None,
            defaults,
        }
    }

    /// 一次性传入所有层，便于调用方按语义组装。
    pub fn from_layers(
        flags: Option<T>,
        env: Option<T>,
        project: Option<T>,
        global: Option<T>,
        defaults: T,
    ) -> Self {
        Self {
            flags,
            env,
            project,
            global,
            defaults,
        }
    }

    pub fn merge(&self) -> MergeResult<T> {
        let mut winner_source: Option<ConfigSource> = None;
        let mut winner_value: Option<T> = None;
        let mut steps = Vec::new();

        // 按优先级扫描，首个存在值即成为 winner，其余记录为 shadowed。
        for source in ConfigSource::precedence_order() {
            let candidate = self.value_for(source);
            match candidate {
                Some(value) => match winner_source {
                    None => {
                        winner_source = Some(source);
                        winner_value = Some(value.clone());
                        steps.push(format!(
                            "source={} decision=selected reason=highest-precedence-present",
                            source
                        ));
                    }
                    Some(winner) => {
                        steps.push(format!(
                            "source={} decision=ignored reason=shadowed-by-{}",
                            source, winner
                        ));
                    }
                },
                None => {
                    steps.push(format!("source={} decision=missing", source));
                }
            }
        }

        let source = winner_source.expect("defaults must always provide a winner");
        steps.push(format!("winner={}", source));

        MergeResult {
            value: winner_value.expect("winner value must exist"),
            source,
            trace: ConfigMergeTrace { steps },
        }
    }

    // 按来源取对应候选值。
    fn value_for(&self, source: ConfigSource) -> Option<&T> {
        match source {
            ConfigSource::Flags => self.flags.as_ref(),
            ConfigSource::Env => self.env.as_ref(),
            ConfigSource::Project => self.project.as_ref(),
            ConfigSource::Global => self.global.as_ref(),
            ConfigSource::Defaults => Some(&self.defaults),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ConfigLayers, ConfigSource};

    #[test]
    fn merge_uses_required_precedence_chain() {
        let layers = ConfigLayers::from_layers(
            Some("from-flags".to_string()),
            Some("from-env".to_string()),
            Some("from-project".to_string()),
            Some("from-global".to_string()),
            "from-defaults".to_string(),
        );

        let merged = layers.merge();
        assert_eq!(merged.value, "from-flags");
        assert_eq!(merged.source, ConfigSource::Flags);
    }

    #[test]
    fn merge_falls_back_to_defaults() {
        let layers = ConfigLayers::new("from-defaults".to_string());
        let merged = layers.merge();

        assert_eq!(merged.value, "from-defaults");
        assert_eq!(merged.source, ConfigSource::Defaults);
    }

    #[test]
    fn merge_explain_trace_mentions_winning_source() {
        let layers = ConfigLayers::from_layers(
            None,
            Some("from-env".to_string()),
            Some("from-project".to_string()),
            None,
            "from-defaults".to_string(),
        );

        let merged = layers.merge();
        assert_eq!(merged.source, ConfigSource::Env);
        assert!(merged
            .trace
            .contains_fragment("source=env decision=selected"));
        assert!(merged.trace.contains_fragment("winner=env"));
    }
}
