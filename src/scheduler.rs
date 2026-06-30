use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::{Local, NaiveTime, Timelike};
use rand::prelude::IndexedRandom;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::{
    config::{NotifyTarget, ScheduleConfig},
    news::{self, DailyNewsDigest},
    notifier::Notifier,
    weather::{self, WeatherSnapshot},
};

/// 一天最多重试拉取快讯的次数
const NEWS_MAX_RETRIES: u32 = 3;
/// 每次重试的间隔秒数
const NEWS_RETRY_DELAY_SECS: u64 = 2;
/// 天气重试次数
const WEATHER_MAX_RETRIES: u32 = 3;
/// 天气重试间隔秒数
const WEATHER_RETRY_DELAY_SECS: u64 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum DailyKind {
    Sign,
    Morning,
    Noon,
    Evening,
    Weather,
}

const SENT_DAILY_FILE: &str = "sent_daily.json";

#[derive(Clone)]
pub struct ScheduleRuntime {
    config: ScheduleConfig,
    notifier: Arc<Notifier>,
    client: reqwest::Client,
    groups: Arc<Vec<i64>>,
    sent_daily: Arc<Mutex<HashSet<(String, DailyKind, i64)>>>,
    sent_daily_path: PathBuf,
    late_reminders: Arc<Mutex<HashMap<i64, Instant>>>,
    news_cache: Arc<Mutex<Option<(String, DailyNewsDigest)>>>,
    weather_cache: Arc<Mutex<Option<(String, WeatherSnapshot)>>>,
    weather_groups: Arc<Vec<i64>>,
    public_base_url: Arc<str>,
}

impl ScheduleRuntime {
    pub fn new(
        config: ScheduleConfig,
        notifier: Arc<Notifier>,
        client: reqwest::Client,
        targets: &[NotifyTarget],
        public_base_url: String,
        sent_daily_override: Option<PathBuf>,
    ) -> Self {
        let groups = targets
            .iter()
            .filter_map(|target| match target {
                NotifyTarget::Group { id } => Some(*id),
                NotifyTarget::Private { .. } => None,
            })
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();

        let weather_groups = if config.weather_groups.is_empty() {
            groups.clone()
        } else {
            config.weather_groups.clone()
        };

        let sent_daily_path = sent_daily_override.unwrap_or_else(|| PathBuf::from(SENT_DAILY_FILE));
        let sent_daily = load_sent_daily(&sent_daily_path);

        Self {
            config,
            notifier,
            client,
            groups: Arc::new(groups),
            sent_daily: Arc::new(Mutex::new(sent_daily)),
            sent_daily_path,
            late_reminders: Arc::new(Mutex::new(HashMap::new())),
            news_cache: Arc::new(Mutex::new(None)),
            weather_cache: Arc::new(Mutex::new(None)),
            weather_groups: Arc::new(weather_groups),
            public_base_url: public_base_url.into(),
        }
    }

    pub fn has_groups(&self) -> bool {
        !self.groups.is_empty()
    }

    pub fn api_token(&self) -> Option<&str> {
        self.config
            .api_token
            .as_deref()
            .filter(|t| !t.trim().is_empty())
    }

    pub fn weather_location(&self) -> Option<&str> {
        self.config
            .weather_location
            .as_deref()
            .filter(|loc| !loc.trim().is_empty())
    }

    pub fn auto_translate(&self) -> bool {
        self.config.auto_translate
    }

    pub fn chat_enabled(&self) -> bool {
        self.config.chat_enabled
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

        // 中午到了先统一拉取一次快讯（缓存到 self.news_cache），失败会重试。
        if self.config.noon_news {
            self.ensure_noon_news(&date, now.time()).await;
        }

        // 早上到了先统一拉取一次天气（缓存到 self.weather_cache），失败会重试。
        if self.config.morning_weather {
            self.ensure_morning_weather(&date, now.time()).await;
        }

        self.run_daily_if_due(
            &date,
            now.time(),
            DailyKind::Weather,
            &self.config.weather_time,
        )
        .await?;
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
            if self.sent_daily.lock().await.contains(&key) {
                continue;
            }

            // 天气只推送到 weather_groups（为空则推所有群）
            if kind == DailyKind::Weather
                && !self.weather_groups.is_empty()
                && !self.weather_groups.contains(&group_id)
            {
                continue;
            }

            let completed = match kind {
                DailyKind::Sign => {
                    tracing::info!(group_id, "signing QQ group");
                    if let Err(error) = self.notifier.sign_group(group_id).await {
                        tracing::warn!(group_id, ?error, "failed to sign QQ group");
                        false
                    } else {
                        tracing::info!(group_id, "signed QQ group");
                        true
                    }
                }
                DailyKind::Morning => {
                    self.send_group_message(group_id, MessageKind::Morning)
                        .await?;
                    true
                }
                DailyKind::Noon => {
                    self.send_noon_message(group_id).await?;
                    true
                }
                DailyKind::Evening => {
                    self.send_group_message(group_id, MessageKind::Evening)
                        .await?;
                    true
                }
                DailyKind::Weather => {
                    self.send_weather_message(group_id).await?;
                    true
                }
            };

            if completed {
                let mut sent = self.sent_daily.lock().await;
                sent.insert(key.clone());
                save_sent_daily(&self.sent_daily_path, &sent);
            }
        }

        Ok(())
    }

    async fn send_group_message(&self, group_id: i64, kind: MessageKind) -> anyhow::Result<()> {
        self.notifier
            .send_direct(&NotifyTarget::Group { id: group_id }, &random_message(kind))
            .await
    }

    async fn send_noon_message(&self, group_id: i64) -> anyhow::Result<()> {
        // 先发温馨问候
        self.notifier
            .send_direct(
                &NotifyTarget::Group { id: group_id },
                &random_message(MessageKind::Noon),
            )
            .await?;

        // 再发快讯（独立一条消息）
        if self.config.noon_news {
            if let Some((_, digest)) = self.news_cache.lock().await.as_ref() {
                self.notifier
                    .send_direct(
                        &NotifyTarget::Group { id: group_id },
                        &news::render_daily_news_message(digest),
                    )
                    .await?;
            } else {
                tracing::warn!(group_id, "noon news cache is empty, skipping news message");
            }
        }

        Ok(())
    }

    /// 确保当天中午快讯已经拉取到缓存中。如果缓存是当天或还没拉取，且已过中午时间，
    /// 则请求接口，失败时最多重试 NEWS_MAX_RETRIES 次。
    async fn ensure_noon_news(&self, date: &str, now: NaiveTime) {
        let Some(noon_time) = parse_time(&self.config.noon_time) else {
            return;
        };
        if now < noon_time {
            return;
        }

        let token = match self
            .config
            .api_token
            .as_deref()
            .filter(|t| !t.trim().is_empty())
        {
            Some(token) => token,
            None => {
                tracing::warn!("no schedule.api_token configured");
                return;
            }
        };

        // 如果缓存已是今天的，不再重复拉取
        {
            let cache = self.news_cache.lock().await;
            if cache
                .as_ref()
                .is_some_and(|(cached_date, _)| cached_date == date)
            {
                return;
            }
        }

        for attempt in 1..=NEWS_MAX_RETRIES {
            match news::fetch_daily_news(&self.client, token).await {
                Ok(digest) => {
                    tracing::info!(date = %digest.date, items = digest.items.len(), "fetched daily AI news");
                    self.news_cache
                        .lock()
                        .await
                        .replace((date.to_string(), digest));
                    return;
                }
                Err(error) => {
                    tracing::warn!(
                        attempt,
                        max_retries = NEWS_MAX_RETRIES,
                        ?error,
                        "failed to fetch daily AI news, will retry"
                    );
                    if attempt < NEWS_MAX_RETRIES {
                        tokio::time::sleep(Duration::from_secs(NEWS_RETRY_DELAY_SECS)).await;
                    }
                }
            }
        }

        tracing::error!(
            "exhausted {} retries for daily AI news, giving up for today",
            NEWS_MAX_RETRIES
        );
    }

    async fn send_weather_message(&self, group_id: i64) -> anyhow::Result<()> {
        let cache = self.weather_cache.lock().await;
        let Some((_, snapshot)) = cache.as_ref() else {
            anyhow::bail!("weather cache is empty, will retry on next tick");
        };
        let base = self.public_base_url.trim_end_matches('/');
        let card_msg = format!(
            "[CQ:image,file={base}/qq/weather.png?{}]",
            weather::weather_card_query(snapshot)
        );
        tracing::info!(group_id, city = %snapshot.city, temp = %snapshot.temperature, "sending weather card");
        drop(cache);
        self.notifier
            .send_direct(&NotifyTarget::Group { id: group_id }, &card_msg)
            .await
    }

    /// 确保当天早上天气已经拉取到缓存中。失败时最多重试 WEATHER_MAX_RETRIES 次。
    async fn ensure_morning_weather(&self, date: &str, now: NaiveTime) {
        let Some(weather_time) = parse_time(&self.config.weather_time) else {
            return;
        };
        if now < weather_time {
            return;
        }

        let token = match self
            .config
            .api_token
            .as_deref()
            .filter(|t| !t.trim().is_empty())
        {
            Some(token) => token,
            None => {
                tracing::warn!("no schedule.api_token configured");
                return;
            }
        };

        let location = match self.config.weather_location.as_deref() {
            Some(loc) if !loc.trim().is_empty() => loc.trim(),
            _ => {
                tracing::warn!("no schedule.weather_location configured");
                return;
            }
        };

        // 如果缓存已是今天的，不再重复拉取
        {
            let cache = self.weather_cache.lock().await;
            if cache
                .as_ref()
                .is_some_and(|(cached_date, _)| cached_date == date)
            {
                return;
            }
        }

        for attempt in 1..=WEATHER_MAX_RETRIES {
            match weather::fetch_weather(&self.client, token, location).await {
                Ok(snapshot) => {
                    tracing::info!(
                        city = %snapshot.city,
                        temp = %snapshot.temperature,
                        "fetched morning weather"
                    );
                    self.weather_cache
                        .lock()
                        .await
                        .replace((date.to_string(), snapshot));
                    return;
                }
                Err(error) => {
                    tracing::warn!(
                        attempt,
                        max_retries = WEATHER_MAX_RETRIES,
                        ?error,
                        "failed to fetch weather, will retry"
                    );
                    if attempt < WEATHER_MAX_RETRIES {
                        tokio::time::sleep(Duration::from_secs(WEATHER_RETRY_DELAY_SECS)).await;
                    }
                }
            }
        }

        tracing::error!(
            "exhausted {} retries for weather, giving up for today",
            WEATHER_MAX_RETRIES
        );
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

fn load_sent_daily(path: &PathBuf) -> HashSet<(String, DailyKind, i64)> {
    let today = Local::now().date_naive().to_string();
    let Ok(content) = fs::read_to_string(path) else {
        return HashSet::new();
    };
    let Ok(entries) = serde_json::from_str::<Vec<(String, DailyKind, i64)>>(&content) else {
        tracing::warn!("failed to parse sent_daily file, starting fresh");
        return HashSet::new();
    };
    entries
        .into_iter()
        .filter(|(date, _, _)| date == &today)
        .collect()
}

fn save_sent_daily(path: &PathBuf, entries: &HashSet<(String, DailyKind, i64)>) {
    let today = Local::now().date_naive().to_string();
    let entries: Vec<_> = entries
        .iter()
        .filter(|(date, _, _)| date == &today)
        .cloned()
        .collect();
    if let Ok(json) = serde_json::to_string(&entries) {
        let _ = fs::write(path, json);
    }
}

fn is_due_now(now: NaiveTime, due_time: NaiveTime, kind: DailyKind) -> bool {
    if matches!(kind, DailyKind::Sign | DailyKind::Noon) {
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::NaiveTime;

    use super::*;
    use crate::bot::MockBot;

    #[tokio::test]
    async fn signs_every_configured_group() {
        let test_path = PathBuf::from(format!("sent_daily_test_{}.json", std::process::id()));
        let _ = std::fs::remove_file(&test_path);

        let bot = Arc::new(MockBot::default());
        let notifier = Arc::new(Notifier::new(bot.clone()));
        let runtime = ScheduleRuntime::new(
            ScheduleConfig::default(),
            notifier,
            reqwest::Client::new(),
            &[
                NotifyTarget::Group { id: 1091113674 },
                NotifyTarget::Group { id: 955437397 },
                NotifyTarget::Private { id: 1692138502 },
            ],
            "http://127.0.0.1:8080".to_string(),
            Some(test_path.clone()),
        );

        runtime
            .run_daily_if_due(
                "2026-06-30",
                NaiveTime::from_hms_opt(9, 1, 0).unwrap(),
                DailyKind::Sign,
                "09:00",
            )
            .await
            .unwrap();

        let mut group_signs = bot.group_signs();
        group_signs.sort_unstable();
        assert_eq!(group_signs, vec![955437397, 1091113674]);

        let _ = std::fs::remove_file(&test_path);
    }

    #[test]
    fn noon_task_catches_up_after_due_time() {
        assert!(is_due_now(
            NaiveTime::from_hms_opt(13, 30, 0).unwrap(),
            NaiveTime::from_hms_opt(12, 0, 0).unwrap(),
            DailyKind::Noon,
        ));
    }
}
