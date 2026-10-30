use serde::Serialize;

/// 后台启动的可视化阶段, 前端展示到 Game 为止, Running 用于收窗。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LaunchStage {
    Preparing,
    Helper,
    Resources,
    Game,
    Running,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchProgress {
    pub stage: LaunchStage,
    pub started_at: u64,
    pub cancelling: bool,
}

impl LaunchProgress {
    pub fn new(started_at: u64) -> Self {
        Self {
            stage: LaunchStage::Preparing,
            started_at,
            cancelling: false,
        }
    }

    /// 阶段只向前推进; 取消期间冻结进度, 避免清单继续跳动。
    pub fn advance(&mut self, stage: LaunchStage) -> bool {
        if self.cancelling || stage <= self.stage {
            return false;
        }
        self.stage = stage;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_only_advance_and_freeze_while_cancelling() {
        let mut progress = LaunchProgress::new(42);
        assert_eq!(progress.stage, LaunchStage::Preparing);
        assert!(!progress.advance(LaunchStage::Preparing));
        assert!(progress.advance(LaunchStage::Helper));
        assert!(!progress.advance(LaunchStage::Preparing));
        progress.cancelling = true;
        assert!(!progress.advance(LaunchStage::Running));
        assert_eq!(progress.stage, LaunchStage::Helper);
    }

    #[test]
    fn progress_serializes_for_the_ui() {
        let progress = LaunchProgress::new(42);
        assert_eq!(
            serde_json::to_value(progress).unwrap(),
            serde_json::json!({
                "stage": "preparing",
                "startedAt": 42,
                "cancelling": false,
            })
        );
        let mut running = LaunchProgress::new(7);
        running.advance(LaunchStage::Running);
        assert_eq!(
            serde_json::to_value(&running).unwrap()["stage"],
            serde_json::json!("running")
        );
    }
}
