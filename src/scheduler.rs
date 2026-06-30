use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::{Local, NaiveTime, Timelike};
use rand::prelude::IndexedRandom;
use tokio::sync::Mutex;

use crate::{
    config::{NotifyTarget, ScheduleConfig},
    notifier::Notifier,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum DailyKind {
    Sign,
    Morning,
    Noon,
    Evening,
}

#[derive(Clone)]
pub struct ScheduleRuntime {
    config: ScheduleConfig,
    notifier: Arc<Notifier>,
    groups: Arc<Vec<i64>>,
    sent_daily: Arc<Mutex<HashSet<(String, DailyKind, i64)>>>,
    late_reminders: Arc<Mutex<HashMap<i64, Instant>>>,
}

impl ScheduleRuntime {
    pub fn new(config: ScheduleConfig, notifier: Arc<Notifier>, targets: &[NotifyTarget]) -> Self {
        let groups = targets
            .iter()
            .filter_map(|target| match target {
                NotifyTarget::Group { id } => Some(*id),
                NotifyTarget::Private { .. } => None,
            })
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();

        Self {
            config,
            notifier,
            groups: Arc::new(groups),
            sent_daily: Arc::new(Mutex::new(HashSet::new())),
            late_reminders: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn has_groups(&self) -> bool {
        !self.groups.is_empty()
    }

    pub async fn run(self: Arc<Self>) {
        loop {
            if let Err(error) = self.tick().await {
                tracing::warn!(?error, "scheduled QQ task failed");
            }
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    }

    pub async fn maybe_send_late_reminder(&self, group_id: i64) -> anyhow::Result<bool> {
        if !self.config.enabled || !self.is_late_hour(Local::now().hour()) {
            return Ok(false);
        }

        let now = Instant::now();
        let mut reminders = self.late_reminders.lock().await;
        if reminders.get(&group_id).is_some_and(|sent_at| {
            now.duration_since(*sent_at).as_secs() < self.config.late_remind_cooldown_secs
        }) {
            return Ok(false);
        }
        reminders.insert(group_id, now);
        drop(reminders);

        self.notifier
            .send_direct(
                &NotifyTarget::Group { id: group_id },
                &random_message(MessageKind::Late),
            )
            .await?;
        Ok(true)
    }

    async fn tick(&self) -> anyhow::Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let now = Local::now();
        let date = now.date_naive().to_string();
        self.run_daily_if_due(
            &date,
            now.time(),
            DailyKind::Morning,
            &self.config.morning_time,
        )
        .await?;
        self.run_daily_if_due(&date, now.time(), DailyKind::Noon, &self.config.noon_time)
            .await?;
        self.run_daily_if_due(
            &date,
            now.time(),
            DailyKind::Evening,
            &self.config.evening_time,
        )
        .await?;
        if self.config.group_sign {
            self.run_daily_if_due(&date, now.time(), DailyKind::Sign, &self.config.sign_time)
                .await?;
        }
        Ok(())
    }

    async fn run_daily_if_due(
        &self,
        date: &str,
        now: NaiveTime,
        kind: DailyKind,
        time: &str,
    ) -> anyhow::Result<()> {
        let Some(due_time) = parse_time(time) else {
            tracing::warn!(%time, "invalid schedule time");
            return Ok(());
        };
        if !is_due_now(now, due_time, kind) {
            return Ok(());
        }

        for group_id in self.groups.iter().copied() {
            let key = (date.to_string(), kind, group_id);
            let mut sent_daily = self.sent_daily.lock().await;
            if !sent_daily.insert(key) {
                continue;
            }
            drop(sent_daily);

            match kind {
                DailyKind::Sign => {
                    if let Err(error) = self.notifier.sign_group(group_id).await {
                        tracing::warn!(group_id, ?error, "failed to sign QQ group");
                    }
                }
                DailyKind::Morning => {
                    self.send_group_message(group_id, MessageKind::Morning)
                        .await?;
                }
                DailyKind::Noon => {
                    self.send_group_message(group_id, MessageKind::Noon).await?;
                }
                DailyKind::Evening => {
                    self.send_group_message(group_id, MessageKind::Evening)
                        .await?;
                }
            }
        }

        Ok(())
    }

    async fn send_group_message(&self, group_id: i64, kind: MessageKind) -> anyhow::Result<()> {
        self.notifier
            .send_direct(&NotifyTarget::Group { id: group_id }, &random_message(kind))
            .await
    }

    fn is_late_hour(&self, hour: u32) -> bool {
        let start = self.config.late_start_hour.min(23);
        let end = self.config.late_end_hour.min(23);
        if start <= end {
            hour >= start && hour <= end
        } else {
            hour >= start || hour <= end
        }
    }
}

fn parse_time(value: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(value, "%H:%M").ok()
}

fn is_due_now(now: NaiveTime, due_time: NaiveTime, kind: DailyKind) -> bool {
    if kind == DailyKind::Sign {
        return now >= due_time;
    }

    let elapsed = now.signed_duration_since(due_time).num_seconds();
    (0..90).contains(&elapsed)
}

#[derive(Debug, Clone, Copy)]
enum MessageKind {
    Morning,
    Noon,
    Evening,
    Late,
}

fn random_message(kind: MessageKind) -> String {
    let mut rng = rand::rng();
    match kind {
        MessageKind::Morning => {
            format_message(&mut rng, MORNING_OPENERS, MORNING_MIDDLES, MORNING_ENDINGS)
        }
        MessageKind::Noon => format_message(&mut rng, NOON_OPENERS, NOON_MIDDLES, NOON_ENDINGS),
        MessageKind::Evening => {
            format_message(&mut rng, EVENING_OPENERS, EVENING_MIDDLES, EVENING_ENDINGS)
        }
        MessageKind::Late => format_message(&mut rng, LATE_OPENERS, LATE_MIDDLES, LATE_ENDINGS),
    }
}

fn format_message(
    rng: &mut impl rand::Rng,
    openers: &[&str],
    middles: &[&str],
    endings: &[&str],
) -> String {
    format!(
        "{}{}{}",
        openers.choose(rng).copied().unwrap_or_default(),
        middles.choose(rng).copied().unwrap_or_default(),
        endings.choose(rng).copied().unwrap_or_default()
    )
}

const MORNING_OPENERS: &[&str] = &[
    "早安，今天也开始啦。",
    "早上好，新的进度条启动。",
    "早安，各位醒醒神。",
    "早呀，给今天开个好头。",
    "早上好，先把精神上线。",
    "早安，愿今天顺手一点。",
    "早，云层再厚也会亮。",
    "早上好，今天也稳稳来。",
    "早安，先喝水再开工。",
    "早呀，别急，慢慢进入状态。",
    "早安，今天适合推进一点点。",
    "早上好，把昨天没做完的轻轻接上。",
];

const MORNING_MIDDLES: &[&str] = &[
    "愿 bug 少一点，灵感多一点。",
    "愿消息不炸，事情不堵。",
    "愿每一步都有回响。",
    "愿手里的事都能顺着走。",
    "愿今天的提交都清清爽爽。",
    "愿咖啡有用，脑子在线。",
    "愿今天的计划不被意外偷走。",
    "愿你碰到的问题都有线索。",
    "愿上午的效率温柔但可靠。",
    "愿该来的好消息早点来。",
    "愿今天的自己比昨天松弛一点。",
    "愿复杂的事慢慢变简单。",
];

const MORNING_ENDINGS: &[&str] = &[
    "开工顺利。",
    "今天加油。",
    "慢慢来，也能很快。",
    "别忘了吃早饭。",
    "先从最小的一件事开始。",
    "保持一点好心情。",
    "稳住，我们能赢。",
    "今天也请多照顾自己。",
    "把节奏握在手里。",
    "祝你一路绿灯。",
];

const NOON_OPENERS: &[&str] = &[
    "午安，上午辛苦了。",
    "中午好，先暂停一下。",
    "午安，该给自己充个电了。",
    "中午到了，别和胃过不去。",
    "午安，上午的风浪先放一放。",
    "中午好，适合认真吃饭。",
    "午安，休息也是正事。",
    "中午啦，把肩膀放松一点。",
    "午安，别让自己一直高负载。",
    "中午好，短暂离线也很好。",
];

const NOON_MIDDLES: &[&str] = &[
    "愿午饭热乎，下午顺手。",
    "愿下午少点突发，多点确定。",
    "愿你能吃饱，也能缓一缓。",
    "愿上午的难题下午有新答案。",
    "愿今天下半场轻一点。",
    "愿待办列表别再偷偷变长。",
    "愿你从饭和阳光里回点血。",
    "愿下午的节奏刚刚好。",
    "愿脑袋重启以后更清醒。",
    "愿该解决的事自然松动。",
];

const NOON_ENDINGS: &[&str] = &[
    "记得吃饭。",
    "午休一下也不亏。",
    "别硬撑，补点能量。",
    "下午继续稳。",
    "吃饱再战。",
    "给自己留二十分钟安静。",
    "午间回血成功。",
    "今天还长，慢慢来。",
    "愿你吃到喜欢的。",
    "轻一点，别绷太紧。",
];

const EVENING_OPENERS: &[&str] = &[
    "晚安，今天收尾啦。",
    "晚上好，该把今天放下了。",
    "晚安，辛苦的一天到站。",
    "晚上好，别让脑子一直加班。",
    "晚安，给今天画个温柔的句号。",
    "夜色到了，节奏可以慢下来。",
    "晚安，今天已经做得够多。",
    "晚上好，把灯调暗一点。",
    "晚安，保存进度，准备休息。",
    "晚上好，别把明天提前透支。",
];

const EVENING_MIDDLES: &[&str] = &[
    "愿你今晚睡得踏实。",
    "愿没完成的事先安静待机。",
    "愿今天的疲惫慢慢退场。",
    "愿梦里没有报错和告警。",
    "愿明天醒来多一点余裕。",
    "愿你能从屏幕里抽身。",
    "愿晚风把焦虑吹淡一点。",
    "愿今天的努力都算数。",
    "愿脑袋自动归档杂事。",
    "愿心情比夜色更柔和。",
];

const EVENING_ENDINGS: &[&str] = &[
    "早点休息。",
    "明天见。",
    "别熬太晚。",
    "今天到这里就很好。",
    "关灯前记得放松眼睛。",
    "愿你好梦。",
    "先睡，剩下的明天再说。",
    "辛苦了，真的。",
    "保存体力也是进度。",
    "今晚不卷。",
];

const LATE_OPENERS: &[&str] = &[
    "已经很晚了。",
    "凌晨还在线呀。",
    "夜深了，提醒一下。",
    "这个点还在忙，辛苦。",
    "现在适合收一收了。",
    "凌晨的脑子容易过热。",
    "夜班模式检测到。",
    "再坚持也别忘了身体。",
];

const LATE_MIDDLES: &[&str] = &[
    "如果不是急事，可以先休息。",
    "明天再看也许会更清楚。",
    "别把睡眠全交出去。",
    "喝口水，活动一下肩颈。",
    "给自己留一点恢复时间。",
    "太晚的决定不一定可靠。",
    "眼睛和脖子都需要下班。",
    "身体不是后台服务，不能一直跑。",
];

const LATE_ENDINGS: &[&str] = &[
    "注意休息。",
    "早点睡吧。",
    "先保存，明天继续。",
    "别太硬扛。",
    "晚一点也要照顾自己。",
    "该休息就休息。",
    "好梦比强撑更重要。",
    "今天可以先到这。",
];
