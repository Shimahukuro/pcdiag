# Windows セキュリティの収集と確認

スキーマ2.3では、以下の基礎8項目に[個別機能33項目](windows-security-coverage.md)を追加した。現在のコレクター状態は合計41項目の取得可否で決定する。旧成果物の8項目のみの入力も受理する。以下の基礎仕様に加え、リンク先の情報源・未収集項目・実機確認手順を参照する。

## 情報源と収集範囲

`windows_security`コレクターはWindows 10/11上で、設定を変更せずに次の情報を取得する。昇格要求は行わず、実行ユーザーの権限で読み取る。

| collection上のフィールド | 情報源 |
|---|---|
| `firewall` | `WSC_SECURITY_PROVIDER_FIREWALL` |
| `automatic_updates` | `WSC_SECURITY_PROVIDER_AUTOUPDATE_SETTINGS` |
| `antivirus` | `WSC_SECURITY_PROVIDER_ANTIVIRUS` |
| `internet_settings` | `WSC_SECURITY_PROVIDER_INTERNET_SETTINGS` |
| `user_account_control` | `WSC_SECURITY_PROVIDER_USER_ACCOUNT_CONTROL` |
| `security_center_service` | `WSC_SECURITY_PROVIDER_SERVICE` |
| `memory_integrity.configured` | `Win32_DeviceGuard.SecurityServicesConfigured` |
| `memory_integrity.running` | `Win32_DeviceGuard.SecurityServicesRunning` |

6カテゴリは`WscGetSecurityProviderHealth`をそれぞれ1回呼び出す。複数カテゴリのフラグを合成すると最も低い状態しか得られないため、合成しない。`wscapi.dll`はSystem32から動的に読み込む。APIがない環境でもCLIの起動と他コレクターの収集を継続する。

HVCIはWindows PowerShellの`Get-CimInstance`からローカルの`root\Microsoft\Windows\DeviceGuard`を照会する。公式の`SecurityServicesConfigured`／`SecurityServicesRunning`の配列に`2`が含まれるかを独立に判定する。前者を`enabled`／`disabled`、後者を`running`／`not_running`へ正規化する。プロパティ欠損、インスタンスなし、空配列を`disabled`や`not_running`で代用しない。`0`と他のサービス番号が混在した応答は不正値とする。他のセキュリティサービス番号は保存しない。

この構成状態はWindowsが報告するHVCIの構成であり、UIトグルやレジストリの単一値を直接読み出したものではない。構成と実行状態が異なっても値を補正したり、再起動が原因だと推定したりしない。

## 状態と失敗の扱い

カテゴリの`good`、`poor`、`snooze`、`not_monitored`はいずれも取得できた観測値であり、取得失敗ではない。`status.json`の`success`は収集成功を意味し、保護状態の正常を意味しない。`not_monitored`は`messages`にも情報コード`wsc_not_monitored`と対象パスを記録する。

| 状況 | collectionの値 | statusのフィールド状態 / 理由コード |
|---|---|---|
| WSCサービス停止（`S_FALSE`） | `null` | `not_collected` / `wsc_service_not_running` |
| WSCアクセス拒否 | `null` | `permission_denied` / `wsc_permission_denied` |
| WSC API非対応 | `null` | `unsupported` / `wsc_unsupported` |
| DLL・関数がない | `null` | `unsupported` / `wsc_library_unavailable`、`wsc_api_unavailable` |
| その他のWSC APIエラー | `null` | `failed` / `wsc_query_failed` |
| 未知の健全性値 | `null` | `invalid_value` / `wsc_health_invalid` |
| Device Guardアクセス拒否・クラス非対応・その他の照会失敗 | HVCIの両項目が`null` | `permission_denied`、`unsupported`、`failed` / `device_guard_query_failed` |
| Device Guardのプロパティ欠損・空配列 | 該当項目のみ`null` | `source_null` / `device_guard_property_unavailable` |
| 矛盾するサービス配列 | 該当項目のみ`null` | `invalid_value` / `device_guard_services_invalid` |
| PowerShell起動・実行失敗、不正なJSON | HVCIの両項目が`null` | `failed` / `device_guard_process_failed`、`invalid_value` / `device_guard_invalid_output` |

DLLの読み込みは、アクセス拒否の場合は`permission_denied`、その他のエラーの場合は`failed`となる。取得不能の`fields`にはcollection上のJSON Pointer、理由コード、および利用可能ならネイティブコードを保存する。HRESULTは符号付き整数で保存する。例外の生テキストやPowerShellの標準エラーは保存しない。

`S_FALSE`時にAPIが出力する`poor`は採用しない。Security Centerサービスカテゴリを含めて該当値を`null`にし、サービス停止を取得不能理由として保持する。他のカテゴリの`poor`と混同しない。

8項目をすべて取得すれば`success`、一部取得なら`partial`、全項目未取得なら`failed`とする。全項目失敗でも個別理由を保持する。ワーカー全体のタイムアウト・異常終了は既存の仕組みに従い、全項目`null`とコレクター単位の理由を保存する。後続コレクターの処理は継続する。既定タイムアウトは120秒、CIMの操作タイムアウトは30秒とし、ワーカーとPowerShell子プロセスは既存のJob Objectによる終了管理を受ける。

## 診断と表示

HTMLに6カテゴリ、HVCIの2項目、根拠パス、未取得理由を表示する。`poor`／`snooze`には注意を表示し、Windows セキュリティの該当画面と組織の管理方針の確認を案内する。カテゴリ内の特定製品や設定を原因と推測しない。未監視と未取得は、正常とも保護無効とも解釈しない。

初期実装では組み込み診断規則を追加せず、診断概要の総合重大度・件数に算入しないことを表示する。HVCIはアプリケーションやドライバーの互換性調査の参考情報であり、有効なだけで原因や異常とは判定しない。無効化は提案せず、関連症状がある場合に設定画面と製品の互換性情報を確認する。設定変更、スキャン、更新、サービス開始は行わない。

## Windows実機での確認

自動テストは全健全性値の変換、`S_FALSE`、権限不足、非対応、部分失敗、HVCI構成と動作の相違、JSON往復変換、表示、旧成果物互換性、ワーカー失敗後の継続を検証する。Windowsでは読み取り専用の実APIスモークテストも実行する。モックテストの成功だけでは通常権限の実機確認済みとはしない。

Windows 10/11の**管理者として起動していない**ターミナルで次を実行する。

```powershell
cargo test -p pcdiag-windows windows_security --locked
cargo build --release --locked -p pcdiag
.\target\release\pcdiag.exe --no-update-check --output .\security-check
```

生成されたセッションについて、次を確認する。

1. `collection/collection.json`の`windows_security`に6カテゴリとHVCIの2項目があり、`status.json`の同名コレクターと整合する。通常権限で取得できた項目は保存され、拒否された項目は理由付きの`null`である。
2. 次の読み取り専用コマンドとHVCIの構成・実行値を照合する。`2`の有無を個別に比較し、取得不能時はエラーを無効状態に読み替えない。

   ```powershell
   Get-CimInstance -Namespace root\Microsoft\Windows\DeviceGuard -ClassName Win32_DeviceGuard |
     Select-Object SecurityServicesConfigured, SecurityServicesRunning
   ```

3. `report/report.html`を開き、6カテゴリの状態、未監視／未取得、理由、構成・実行の区別を確認する。Windows セキュリティ画面も参照するが、製品別表示と集約カテゴリを同一視しない。
4. 既に存在する、または管理者が用意した検証環境で、サービス停止・アクセス拒否・HVCI非対応を確認する。検証のために運用端末の保護を無効にしない。サービス停止時に`poor`が保存されないこと、他の取得値と後続の`clock`以降の結果が残ることを確認する。
5. Debug版では次の遅延フックによりタイムアウトを再現できる。`windows_security`が全項目`null`と`collector_timeout`になり、後続コレクターとレポート生成が完了することを確認する。

   ```powershell
   cargo build --locked -p pcdiag
   $env:PCDIAG_TEST_DELAY_COLLECTOR = 'windows_security'
   $env:PCDIAG_TEST_DELAY_MS = '2500'
   .\target\debug\pcdiag.exe --no-update-check --output .\security-timeout-check --collector-timeout windows_security=1
   Remove-Item Env:PCDIAG_TEST_DELAY_COLLECTOR, Env:PCDIAG_TEST_DELAY_MS
   ```

実機確認結果にはOS、通常／管理者権限、各情報源の取得可否、確認できなかった条件を記録する。未加工の診断成果物を公開リポジトリへ追加しない。

## 公式資料

- [WscGetSecurityProviderHealth](https://learn.microsoft.com/en-us/windows/win32/api/wscapi/nf-wscapi-wscgetsecurityproviderhealth)
- [WSC_SECURITY_PROVIDER](https://learn.microsoft.com/en-us/windows/win32/api/wscapi/ne-wscapi-wsc_security_provider)
- [WSC_SECURITY_PROVIDER_HEALTH](https://learn.microsoft.com/en-us/windows/win32/api/wscapi/ne-wscapi-wsc_security_provider_health)
- [メモリ整合性とWin32_DeviceGuardの状態照会](https://learn.microsoft.com/en-us/windows/security/hardware-security/enable-virtualization-based-protection-of-code-integrity)
