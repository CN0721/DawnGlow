//! 应用数据模型。

/// 页面。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    Forecast,
    Map,
    Settings,
    Font,
    Version,
}

/// 地点。
#[derive(Clone, Debug, Default)]
pub struct Location {
    /// 展示名（城市/地名，或「地图选点」）
    pub name: String,
    pub lat: f64,
    pub lon: f64,
}

impl Location {
    pub fn coord_text(&self) -> String {
        format!("{:.4}, {:.4}", self.lat, self.lon)
    }
}

/// 事件类型。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EventKind {
    Sunrise,
    Sunset,
}

impl EventKind {
    pub fn label(self) -> &'static str {
        match self {
            EventKind::Sunrise => "朝霞",
            EventKind::Sunset => "晚霞",
        }
    }

    pub fn emoji(self) -> &'static str {
        match self {
            EventKind::Sunrise => "🌄",
            EventKind::Sunset => "🌇",
        }
    }
}

/// 云型分类。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum CloudType {
    /// 高云主导（卷云/卷积云，散射最佳）
    HighDominant,
    /// 低云主导（层积云）
    LowDominant,
    /// 混合云层
    #[default]
    Mixed,
    /// 晴空
    Clear,
    /// 阴天
    Overcast,
}

impl CloudType {
    pub fn label(self) -> &'static str {
        match self {
            CloudType::HighDominant => "高云主导",
            CloudType::LowDominant => "低云主导",
            CloudType::Mixed => "混合云层",
            CloudType::Clear => "晴空少云",
            CloudType::Overcast => "阴天蔽日",
        }
    }

    pub fn desc(self) -> &'static str {
        match self {
            CloudType::HighDominant => "卷云/卷积云，红光散射最强",
            CloudType::LowDominant => "层积云，效果良好",
            CloudType::Mixed => "多层云，纹理丰富",
            CloudType::Clear => "无云散射，色彩平淡",
            CloudType::Overcast => "光线被遮挡，难以出彩",
        }
    }
}

/// 评分因子明细。
#[derive(Clone, Debug, Default)]
pub struct Factor {
    pub label: String,
    pub value: String,
    /// 对总分的贡献（可正可负）
    pub delta: f32,
}

/// 单个事件（日出/日落）的评分。
#[derive(Clone, Debug, Default)]
pub struct EventScore {
    pub score: f32,
    pub confidence: f32,
    pub cloud_low: Option<f32>,
    pub cloud_mid: Option<f32>,
    pub cloud_high: Option<f32>,
    pub cloud_total: Option<f32>,
    pub humidity: Option<f32>,
    pub visibility_km: Option<f32>,
    pub aod: Option<f32>,
    pub rain_prob: Option<f32>,
    pub cloud_type: CloudType,
    pub factors: Vec<Factor>,
    /// 各模式的独立评分（融合模式下不止一项），用于展示模型分歧。
    pub model_scores: Vec<(String, f32)>,
    /// 模式间分歧（0..1）：越大说明 GFS 与 ECMWF 越不一致。
    pub spread: f32,
}

impl EventScore {
    /// 鲜艳度等级文案。
    pub fn grade(&self) -> &'static str {
        let s = self.score;
        if s >= 0.75 {
            "绝佳 · 火烧云级别"
        } else if s >= 0.6 {
            "很好 · 值得出工"
        } else if s >= 0.45 {
            "一般 · 可拍可不拍"
        } else if s >= 0.3 {
            "偏弱 · 别抱太大期望"
        } else if s >= 0.15 {
            "较差"
        } else {
            "基本无望"
        }
    }
}

/// 单日预报。
#[derive(Clone, Debug, Default)]
pub struct DayForecast {
    /// YYYY-MM-DD
    pub date: String,
    /// "HH:MM"（当地时间）
    pub sunrise: Option<String>,
    pub sunset: Option<String>,
    pub sunrise_score: EventScore,
    pub sunset_score: EventScore,
}

impl DayForecast {
    pub fn score_for(&self, kind: EventKind) -> &EventScore {
        match kind {
            EventKind::Sunrise => &self.sunrise_score,
            EventKind::Sunset => &self.sunset_score,
        }
    }

    pub fn time_for(&self, kind: EventKind) -> Option<&str> {
        match kind {
            EventKind::Sunrise => self.sunrise.as_deref(),
            EventKind::Sunset => self.sunset.as_deref(),
        }
    }

    /// 该日更优的事件。
    pub fn best_event(&self) -> EventKind {
        if self.sunrise_score.score >= self.sunset_score.score {
            EventKind::Sunrise
        } else {
            EventKind::Sunset
        }
    }
}

/// 一次完整预报结果。
#[derive(Clone, Debug, Default)]
pub struct ForecastBundle {
    pub location: Location,
    pub days: Vec<DayForecast>,
    /// 数据地点的 UTC 偏移（秒），用于显示本地更新时间。
    pub utc_offset_seconds: i64,
}
