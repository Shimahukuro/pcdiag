use serde::{Deserialize, Serialize};

use super::memory::deserialize_required_nullable;

/// Category health reported by Windows Security Center, not product-specific settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowsSecurityCollection {
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub firewall: Option<SecurityHealth>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub automatic_updates: Option<SecurityHealth>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub antivirus: Option<SecurityHealth>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub internet_settings: Option<SecurityHealth>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub user_account_control: Option<SecurityHealth>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub security_center_service: Option<SecurityHealth>,
    pub memory_integrity: MemoryIntegrity,
    /// Optional extension; older artifacts omit the entire map. Null requires a status reason.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub details: std::collections::BTreeMap<String, Option<String>>,
}

impl WindowsSecurityCollection {
    pub const DETAIL_FIELDS: &'static [(&'static str, &'static str, &'static str)] = &[
        (
            "smart_app_control",
            "アプリとブラウザー コントロール",
            "スマート アプリ コントロール",
        ),
        (
            "defender_mode",
            "ウイルスと脅威の防止",
            "Defender 動作モード",
        ),
        (
            "antivirus_enabled",
            "ウイルスと脅威の防止",
            "Defender ウイルス対策",
        ),
        (
            "realtime_protection",
            "ウイルスと脅威の防止",
            "リアルタイム保護",
        ),
        ("behavior_monitor", "ウイルスと脅威の防止", "動作監視"),
        (
            "ioav_protection",
            "ウイルスと脅威の防止",
            "ダウンロード・添付ファイルの保護",
        ),
        ("tamper_protection", "ウイルスと脅威の防止", "改ざん防止"),
        (
            "signature_version",
            "ウイルスと脅威の防止",
            "セキュリティ インテリジェンス バージョン",
        ),
        (
            "signature_updated",
            "ウイルスと脅威の防止",
            "セキュリティ インテリジェンス 最終更新（UTC）",
        ),
        (
            "signatures_outdated",
            "ウイルスと脅威の防止",
            "Defender 定義の期限切れ",
        ),
        (
            "quick_scan_start",
            "ウイルスと脅威の防止",
            "クイック スキャン開始（UTC）",
        ),
        (
            "quick_scan_end",
            "ウイルスと脅威の防止",
            "クイック スキャン終了（UTC）",
        ),
        (
            "full_scan_start",
            "ウイルスと脅威の防止",
            "フル スキャン開始（UTC）",
        ),
        (
            "full_scan_end",
            "ウイルスと脅威の防止",
            "フル スキャン終了（UTC）",
        ),
        (
            "active_threat_count",
            "ウイルスと脅威の防止",
            "Defender が報告するアクティブな脅威の件数",
        ),
        (
            "controlled_folder_access",
            "ウイルスと脅威の防止",
            "ランサムウェア防止: コントロールされたフォルダー アクセス",
        ),
        (
            "pua_protection",
            "アプリとブラウザー コントロール",
            "Defender 望ましくないアプリのブロック",
        ),
        (
            "network_protection",
            "アプリとブラウザー コントロール",
            "Defender ネットワーク保護",
        ),
        (
            "firewall_domain",
            "ファイアウォールとネットワーク保護",
            "ドメイン プロファイル",
        ),
        (
            "firewall_private",
            "ファイアウォールとネットワーク保護",
            "プライベート プロファイル",
        ),
        (
            "firewall_public",
            "ファイアウォールとネットワーク保護",
            "パブリック プロファイル",
        ),
        (
            "active_firewall_profiles",
            "ファイアウォールとネットワーク保護",
            "アクティブなプロファイル",
        ),
        (
            "exploit_dep",
            "アプリとブラウザー コントロール",
            "Exploit protection: DEP（システム設定）",
        ),
        (
            "exploit_cfg",
            "アプリとブラウザー コントロール",
            "Exploit protection: CFG（システム設定）",
        ),
        (
            "exploit_aslr_bottom_up",
            "アプリとブラウザー コントロール",
            "Exploit protection: Bottom-up ASLR（システム設定）",
        ),
        (
            "exploit_aslr_force",
            "アプリとブラウザー コントロール",
            "Exploit protection: 強制 ASLR（システム設定）",
        ),
        (
            "exploit_sehop",
            "アプリとブラウザー コントロール",
            "Exploit protection: SEHOP（システム設定）",
        ),
        (
            "exploit_heap",
            "アプリとブラウザー コントロール",
            "Exploit protection: ヒープ整合性（システム設定）",
        ),
        ("tpm_present", "デバイス セキュリティ", "TPM の存在"),
        ("tpm_ready", "デバイス セキュリティ", "TPM の準備完了"),
        ("tpm_enabled", "デバイス セキュリティ", "TPM の有効状態"),
        (
            "tpm_locked_out",
            "デバイス セキュリティ",
            "TPM ロックアウト",
        ),
        ("secure_boot", "デバイス セキュリティ", "セキュア ブート"),
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecurityHealth {
    Good,
    Poor,
    Snooze,
    NotMonitored,
}

/// Configuration and runtime observations are independent; neither implies the other.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryIntegrity {
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub configured: Option<MemoryIntegrityConfiguration>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub running: Option<MemoryIntegrityRunningState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryIntegrityConfiguration {
    Enabled,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryIntegrityRunningState {
    Running,
    NotRunning,
}
