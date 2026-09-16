# Windows セキュリティ画面との対応（スキーマ2.3）

2026-09-16に提示された9枚の画面と実装を照合した一覧。画面上の説明、設定変更ボタン、ヘルプ・プライバシーのリンクは観測値ではないため収集対象にしない。

## 実装した不足項目

`windows_security.details` に33項目を追加。キーと日本語名の一覧は `WindowsSecurityCollection::DETAIL_FIELDS` を正本とする。既存のWSC 6項目とHVCI 2項目を合わせて41項目となる。

| 画面 | 今回追加した観測情報 | 情報源 |
|---|---|---|
| ウイルスと脅威の防止 | Defender動作モード、ウイルス対策、リアルタイム保護、動作監視、添付ファイル保護、改ざん防止 | Get-MpComputerStatus |
| 同上・更新 | 定義バージョン、最終更新UTC日時、期限切れフラグ | Get-MpComputerStatus |
| 同上・スキャン | クイック／フルの開始・終了UTC日時 | Get-MpComputerStatus |
| 同上・現在の脅威 | アクティブな脅威の件数（パス・ユーザー名・脅威本文は保存しない） | Get-MpThreat.IsActive |
| 同上・ランサムウェア | コントロールされたフォルダーアクセスの無効・有効・監査・ディスク変更限定モード | Get-MpPreference |
| アプリとブラウザー コントロール | スマート アプリ コントロールの有効・無効・評価 | Get-MpComputerStatus.SmartAppControlState |
| 同上・評価ベース保護 | Defender PUA保護、ネットワーク保護 | Get-MpPreference |
| 同上・Exploit protection | システム設定のDEP、CFG、Bottom-up ASLR、強制ASLR、SEHOP、ヒープ整合性 | Get-ProcessMitigation -System |
| ファイアウォールとネットワーク保護 | Domain/Private/Publicの実効設定、アクティブなプロファイル（複数可） | Get-NetFirewallProfile -PolicyStore ActiveStore / INetFwPolicy2.CurrentProfileTypes |
| デバイス セキュリティ | TPMの存在・準備完了・有効・ロックアウト、セキュアブートの有効状態 | Get-Tpm / Confirm-SecureBootUEFI |

SACはレジストリ単独値による推定ではなくDefenderが報告する状態を使用する。コマンドやプロパティがない環境では未取得になる。Defenderの結果は他社製品の状態を表さない。スキャン日時の欠損や初期値は未取得であり、「スキャン未実施」と断定しない。NOTSETはExploit protectionの明示設定なしであり、無効や脆弱を意味しない。

## 収集できない／今回対象外の情報

以下は「技術的に絶対取得不可能」という意味ではない。現在の読み取り専用コレクターで信頼できる取得方式を確立していないものと、Issue #28の収集オプション待ちを区別する。

| 画面・項目 | 状況と理由 |
|---|---|
| セキュリティの概要の8タイルの総合表示 | WSCの6カテゴリとは同一ではない。独自の「操作不要」判定は作らない |
| 最後のスキャンの種類・検査ファイル数・検出数・所要時間 | UIの最後のスキャンに対応する一貫した公開プロパティを確立していない。今回保存する開始・終了日時と混同しない |
| 保護履歴本文・許可された脅威・検出対象パス | 機微情報オプション（Issue #28）待ち。現在の脅威件数だけでは履歴なしとは判断しない |
| Microsoftアカウント、Windows Hello、動的ロック | ユーザー別収集方式・対象ユーザーの扱いをIssue #28で設計する。昇格ユーザーとログオンユーザーの取り違えを防ぐ必要がある。UACで代用しない |
| SmartScreen、フィッシング防止、Storeアプリ保護などの実効状態 | ユーザー設定・管理ポリシー・既定値の優先順位を含む取得方式未確立。PUAやWSC internet_settingsで代用しない |
| Exploit protectionのアプリ別例外 | 今回はシステム設定のみ。実行ファイル情報を含むため収集範囲の追加設計が必要 |
| セキュアブート証明書の更新完了 | Secure Bootの有効状態だけでは判定不可。更新進行状態の別実装が必要 |
| 標準ハードウェアセキュリティ要件の総合判定 | TPM/HVCI/Secure BootだけではWindows UIと同じ判定を再現できないため未判定 |
| パフォーマンスと正常性の最終スキャン・記憶域・アプリ・時刻サービスの各緑マーク | Windows Security固有の評価を取得していない。既存のstorage/runtime_environment/clockの観測結果は利用可能だが同一視しない |
| ファミリーの家族構成・画面時間・活動レポート | クラウド認証と個人情報を伴う。Issue #28の対象、外部サービスへの自動接続は行わない |
| OneDriveのランサムウェア復旧設定 | ユーザー・クラウド連携を含み、今回のCFA状態からは取得できない |

## データと失敗の扱い

新規収集ではdetailsの全33キーを保存する。値は短い文字列、取得不能はnullとし、status.jsonの同じJSON Pointerにpermission_denied / unsupported / source_null / failed / invalid_valueを記録する。false、disabled、0件は成功した観測値。情報源オブジェクトや例外本文を丸ごと保存しない。

2.0～2.2成果物は引き続き読める。details自体が存在しない場合は未収集と表示する。出力スキーマは2.3。ワーカー全体のタイムアウト時は既存のコレクター単位の理由を使用する。HTMLは初期クローズのアコーディオンを維持し、未収集の領域も明記する。

## 実機確認

Windows 11の通常権限と管理者権限で以下を実行する。Windows 10ではSAC等の非対応・欠損が無効表示にならないことを確認する。

```powershell
cargo test --workspace --locked
cargo build --release --locked -p pcdiag
.\target\release\pcdiag.exe --no-update-check --output .\security-detail-check
Get-MpComputerStatus | Select-Object SmartAppControlState, RealTimeProtectionEnabled, AntivirusSignatureLastUpdated
Get-NetFirewallProfile -PolicyStore ActiveStore | Select-Object Name, Enabled
Get-ProcessMitigation -System
Get-Tpm | Select-Object TpmPresent, TpmReady, TpmEnabled, LockedOut
Confirm-SecureBootUEFI
```

collection.json/details、status.jsonの理由、HTMLを同じ権限の照会結果と照合する。通常ユーザーで拒否される項目、Defender非搭載・無効環境、スキャン履歴なし、旧成果物、ワーカータイムアウトも確認する。保護設定の変更は検証に不要。macOSでのテスト成功やWindows向けcargo checkは実機確認の代用にはならない。

## 今回の検証結果

- macOS上のワークスペーステスト215件が成功。
- Clippy（全ターゲット、警告をエラー扱い）が成功。
- pcdiag-windowsのWindows MSVC向けチェック（テストコード含む）が成功。
- PowerShell 7.4 Linuxコンテナでモックテスト成功。SAC全状態、未知値、false、0件、権限不足、スキャン日時初期値、機微情報の非保存を確認。Windowsでは同じテストをWindows PowerShellで実行するテストを追加したが、この環境からは未実行。
- CLI全体のWindows向けチェックは依存ringが必要とするWindows Cヘッダー（assert.h）がこのMacにないため未完了。
- 実PCのWindows APIとUIの照合、Windows PowerShell 5.1実行、実収集からHTML生成までの確認は未実施。macOS上でのCLI収集は仕様どおりWindows専用として拒否された。

## 公式情報源

- https://learn.microsoft.com/en-us/powershell/module/defender/get-mpcomputerstatus
- https://learn.microsoft.com/en-us/powershell/module/defender/get-mppreference
- https://learn.microsoft.com/en-us/powershell/module/defender/get-mpthreat
- https://learn.microsoft.com/en-us/powershell/module/netsecurity/get-netfirewallprofile
- https://learn.microsoft.com/en-us/windows/win32/api/netfw/nf-netfw-inetfwpolicy2-get_currentprofiletypes
- https://learn.microsoft.com/en-us/powershell/module/processmitigations/get-processmitigation
- https://learn.microsoft.com/en-us/powershell/module/trustedplatformmodule/get-tpm
- https://learn.microsoft.com/en-us/powershell/module/secureboot/confirm-securebootuefi
