# Issue #2 要求トレーサビリティ

## 目的

Issue #2「CLI基本仕様」に記載された要求が、データモデル、診断処理、HTMLレポートへ漏れなく反映されることを確認する。

本書は設計進行中の対応表である。JSON Pointerは現在の設計案であり、カテゴリ設計時に変更できる。

## 判定区分

- `診断対象`: 収集値を診断ルールで評価する。
- `診断根拠`: 単独では診断しないが、別の評価で使用する。
- `表示のみ`: 主にシステム構成の説明としてレポートへ表示する。
- `内部管理`: 成果物または処理の管理に使用する。
- `対象外`: 初期実装では扱わない。
- `禁止`: 機密情報除外方針によりスキーマへ含めない。

## 実行状態

| Issue #2の要求 | 保存先・参照先 | status.json | diagnosis.json | 用途 |
|---|---|---|---|---|
| `pcdiag`のバージョン | `manifest.json:/tool/version` | — | — | 内部管理 |
| 収集開始・完了日時 | `manifest.json:/started_at`, `/completed_at` | — | — | 内部管理 |
| 処理時間 | `manifest.json:/duration_ms` | コレクターごとにも記録 | — | 内部管理 |
| バンドルスキーマ | `manifest.json:/manifest_schema_version` | — | — | 内部管理 |
| データスキーマ | `manifest.json:/artifact_schema_version` | — | — | 内部管理 |
| OSの種類 | `/windows`配下 | `windows` | 必要に応じて根拠 | 表示のみ・診断根拠 |
| システムアーキテクチャ | `/windows/architecture` | `windows` | 互換性判定の根拠候補 | 表示のみ・診断根拠 |
| 管理者権限 | `/windows/privilege`候補 | `environment`候補 | 未評価理由の根拠 | 診断根拠 |
| コレクター実行状態 | — | `/collectors/*/status` | 未評価理由の根拠 | 内部管理 |
| コレクター処理時間 | — | `/collectors/*/duration_ms` | — | 内部管理 |

## Windows

| 要求 | collection.json候補 | コレクター | 診断上の用途 | レポート |
|---|---|---|---|---|
| エディション | `/windows/edition` | `windows` | 表示のみ | システム概要 |
| バージョン | `/windows/version` | `windows` | 診断根拠候補 | システム概要 |
| ビルド番号 | `/windows/build_number` | `windows` | 診断根拠候補 | システム概要 |
| OSアーキテクチャ | `/windows/architecture` | `windows` | 診断根拠候補 | システム概要 |
| 最終起動時刻 | `/windows/booted_at` | `windows` | 稼働時間検証 | システム概要 |
| 稼働時間 | `/windows/uptime_ms` | `windows` | 診断根拠候補 | システム概要 |
| システム時刻 | `/clock/system_time_utc` | `clock` | 時計診断 | 時計情報 |
| UTCオフセット | `/clock/utc_offset_minutes` | `clock` | 時計診断の補助 | 時計情報 |
| 起動方式 | `/windows/boot_mode` | `windows` | 診断根拠候補 | システム概要 |
| Windowsのシステム状態 | `/windows/status` | `windows` | 診断対象候補 | システム概要 |
| Windows Timeサービス | `/clock/windows_time_service` | `clock` | 診断対象 | 時計情報 |
| ハードウェアRTC取得可否 | `/clock/hardware_clock` | `clock` | 診断根拠 | 時計情報 |

## CPU

| 要求 | collection.json候補 | コレクター | 診断上の用途 | レポート |
|---|---|---|---|---|
| メーカー | `/cpu/packages/*/manufacturer` | `cpu` | 表示のみ | CPU |
| モデル名 | `/cpu/packages/*/model` | `cpu` | 表示・診断根拠 | CPU |
| 物理CPU数 | `/cpu/packages`の件数 | `cpu` | 表示のみ | CPU |
| 物理コア数 | `/cpu/packages/*/physical_cores` | `cpu` | 診断根拠候補 | CPU |
| 論理プロセッサー数 | `/cpu/packages/*/logical_processors` | `cpu` | 診断根拠候補 | CPU |
| アーキテクチャ | `/cpu/packages/*/architecture` | `cpu` | 診断根拠候補 | CPU |
| Windows上の状態 | `/cpu/packages/*/status` | `cpu` | 診断対象候補 | CPU |
| 基本機能 | `/cpu/packages/*/features` | `cpu` | 診断根拠候補 | CPU |

## メモリ

| 要求 | collection.json | コレクター | diagnosis.json | レポート |
|---|---|---|---|---|
| 物理メモリ総容量 | `/memory/physical/total_bytes` | `memory` | 診断根拠 | メモリ |
| 使用可能容量 | `/memory/physical/available_bytes` | `memory` | 診断対象 | メモリ |
| 使用中容量 | 総量と使用可能量から導出 | `memory` | 派生根拠 | メモリ |
| メモリ使用率 | `/memory/physical/load_percent` | `memory` | 診断対象 | メモリ |
| Windows認識容量 | `/memory/physical/total_bytes` | `memory` | 診断根拠 | メモリ |

## ストレージ

| 要求 | collection.json候補 | コレクター | 診断上の用途 | レポート |
|---|---|---|---|---|
| メーカー | `/storage/disks/*/manufacturer` | `storage` | 表示のみ | ストレージ |
| モデル名 | `/storage/disks/*/model` | `storage` | 表示・診断根拠 | ストレージ |
| ディスク種別 | `/storage/disks/*/device_type` | `storage` | 診断根拠 | ストレージ |
| HDD・SSD・NVMe | `/storage/disks/*/media_type` | `storage` | 診断根拠 | ストレージ |
| 総容量 | `/storage/disks/*/capacity_bytes` | `storage` | 診断根拠 | ストレージ |
| 接続方式 | `/storage/disks/*/bus_type` | `storage` | 診断根拠 | ストレージ |
| Windows上の状態 | `/storage/disks/*/status` | `storage` | 診断対象 | ストレージ |
| SMART取得可否 | `/storage/disks/*/smart/available` | `storage` | 未評価理由・診断根拠 | ストレージ |
| ドライブ文字 | `/storage/volumes/*/drive_letter` | `storage` | 表示のみ | ボリューム |
| ファイルシステム | `/storage/volumes/*/file_system` | `storage` | 診断根拠 | ボリューム |
| ボリューム総容量 | `/storage/volumes/*/total_bytes` | `storage` | 診断根拠 | ボリューム |
| 使用容量 | `/storage/volumes/*/used_bytes`または導出 | `storage` | 診断根拠 | ボリューム |
| 空き容量 | `/storage/volumes/*/available_bytes` | `storage` | 診断対象 | ボリューム |
| ボリューム状態 | `/storage/volumes/*/status` | `storage` | 診断対象候補 | ボリューム |

## BIOS・UEFI

| 要求 | collection.json候補 | コレクター | 診断上の用途 | レポート |
|---|---|---|---|---|
| ベンダー | `/firmware/vendor` | `firmware` | 表示のみ | ファームウェア |
| バージョン | `/firmware/version` | `firmware` | 診断根拠候補 | ファームウェア |
| リリース日 | `/firmware/release_date` | `firmware` | 診断根拠候補 | ファームウェア |
| BIOS・UEFI区分 | `/firmware/interface_type` | `firmware` | 診断根拠 | ファームウェア |
| Secure Boot | `/firmware/secure_boot_enabled` | `firmware` | 診断対象候補 | ファームウェア |
| ファームウェア状態 | `/firmware/status` | `firmware` | 診断対象候補 | ファームウェア |

## デバイス

| 要求 | collection.json候補 | コレクター | 診断上の用途 | レポート |
|---|---|---|---|---|
| デバイス種別 | `/devices/*/class` | `devices` | 分類 | デバイス |
| メーカー | `/devices/*/manufacturer` | `devices` | 表示のみ | デバイス |
| モデル名 | `/devices/*/model` | `devices` | 表示・診断根拠 | デバイス |
| 現在・過去の存在状態 | `/devices/*/presence` | `devices` | 診断根拠 | 現在／非接続を分離 |
| Windows上の状態 | `/devices/*/status` | `devices` | 診断対象 | デバイス |
| 問題コード | `/devices/*/problem_code` | `devices` | 診断対象 | デバイス |
| ドライバー提供元 | `/devices/*/driver/provider` | `devices` | 診断根拠 | デバイス |
| ドライバーバージョン | `/devices/*/driver/version` | `devices` | 診断根拠 | デバイス |
| ドライバー日付 | `/devices/*/driver/date` | `devices` | 診断根拠 | デバイス |
| ドライバー署名 | `/devices/*/driver/signed` | `devices` | 診断対象候補 | デバイス |
| USB Vendor ID | `/devices/*/usb/vendor_id` | `devices` | 製品分類 | デバイス |
| USB Product ID | `/devices/*/usb/product_id` | `devices` | 製品分類 | デバイス |

## GPU

| 要求 | collection.json候補 | コレクター | 診断上の用途 | レポート |
|---|---|---|---|---|
| GPU数 | `/gpus`の件数 | `gpu` | 構成判定 | GPU |
| メーカー | `/gpus/*/manufacturer` | `gpu` | 表示・診断根拠 | GPU |
| モデル名 | `/gpus/*/model` | `gpu` | 表示・診断根拠 | GPU |
| GPU区分 | `/gpus/*/adapter_type` | `gpu` | 診断根拠 | GPU |
| プライマリGPU | `/gpus/*/primary` | `gpu` | 診断根拠 | GPU |
| 存在状態 | `/gpus/*/presence` | `gpu` | 診断根拠 | GPU |
| Windows上の状態 | `/gpus/*/status` | `gpu` | 診断対象 | GPU |
| 問題コード | `/gpus/*/problem_code` | `gpu` | 診断対象 | GPU |
| PCI Vendor ID | `/gpus/*/pci/vendor_id` | `gpu` | 製品分類 | GPU |
| PCI Device ID | `/gpus/*/pci/device_id` | `gpu` | 製品分類 | GPU |
| Subsystem ID | `/gpus/*/pci/subsystem_id` | `gpu` | 製品分類 | GPU |
| Revision ID | `/gpus/*/pci/revision_id` | `gpu` | 製品分類 | GPU |
| 専用VRAM | `/gpus/*/memory/dedicated_bytes` | `gpu` | 診断根拠 | GPU |
| 共有メモリ | `/gpus/*/memory/shared_bytes` | `gpu` | 診断根拠 | GPU |
| VRAM使用量 | `/gpus/*/memory/dedicated_used_bytes` | `gpu` | 診断対象候補 | GPU |
| ドライバー提供元 | `/gpus/*/driver/provider` | `gpu` | 診断根拠 | GPU |
| ドライバーバージョン | `/gpus/*/driver/version` | `gpu` | 診断根拠 | GPU |
| ドライバー日付 | `/gpus/*/driver/date` | `gpu` | 診断根拠 | GPU |
| ドライバーモデル | `/gpus/*/driver/model` | `gpu` | 診断根拠 | GPU |
| WDDM | `/gpus/*/driver/wddm_version` | `gpu` | 診断根拠 | GPU |
| ドライバー署名 | `/gpus/*/driver/signed` | `gpu` | 診断対象候補 | GPU |
| Basic Display Adapter | `/gpus/*/driver/basic_display_adapter` | `gpu` | 診断対象 | GPU |
| DXGI認識 | `/gpus/*/graphics/dxgi_available` | `gpu` | 診断対象 | GPU |
| HWアクセラレーション | `/gpus/*/graphics/hardware_acceleration` | `gpu` | 診断対象 | GPU |
| Direct3D機能レベル | `/gpus/*/graphics/direct3d_feature_levels` | `gpu` | 診断根拠 | GPU |
| DirectX 12 | `/gpus/*/graphics/directx_12_supported` | `gpu` | 診断根拠 | GPU |

## 機密情報として禁止するフィールド

次の情報は初期スキーマへ含めない。

| 禁止情報 | スキーマ上の扱い |
|---|---|
| コンピューター名 | フィールドを作らない |
| ユーザー名、アカウント、メール | フィールドを作らない |
| IPアドレス、MACアドレス | フィールドを作らない |
| Wi-Fi SSID、認証情報 | フィールドを作らない |
| Windowsプロダクトキー | フィールドを作らない |
| 各種シリアル番号 | フィールドを作らない |
| PnPインスタンスID、コンテナID | フィールドを作らない |
| 詳細な接続パス | フィールドを作らない |
| ファイル名、絶対パス、内容 | フィールドを作らない |
| 環境変数 | フィールドを作らない |
| アプリケーション、プロセス一覧 | フィールドを作らない |
| イベントログ、任意レジストリ | 初期対象外 |
| 生のAPI・コマンド出力 | 初期対象外 |
| クラッシュ・メモリダンプ | 初期対象外 |
| GPUメモリ、画面内容 | フィールドを作らない |

## 完了判定

各要求行について、次のいずれかが確定した時点で網羅済みとする。

- 正式な保存先と型が決まっている。
- 派生値として計算方法が決まっている。
- 診断ルールでの用途が決まっている。
- 表示専用情報として分類されている。
- 初期対象外として理由が記録されている。
- 機密情報としてスキーマから禁止されている。

