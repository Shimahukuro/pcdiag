# コレクターのタイムアウト

## 適用単位と実行方式

タイムアウトはAPI呼び出し単位ではなく、`status.json`へ結果を記録するコレクター単位で適用する。各コレクターは`pcdiag.exe`の内部ワーカープロセスとして1つずつ順番に実行し、親プロセスが完了、中断、制限時間超過を監視する。

Windowsでは各ワーカーを専用のJob Objectへ割り当て、`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`を設定する。タイムアウトまたは利用者による中断ではワーカーを終了し、Job Objectを閉じることで、ワーカーが起動したPowerShellなどの子孫プロセスも終了対象にする。同一プロセス内のスレッドを強制終了する方式は使用しない。

## 既定時間

| コレクター | 既定時間 |
|---|---:|
| `windows`、`clock`、`cpu`、`firmware`、`memory` | 10秒 |
| `gpu`、`devices`、`physical_disks`、`partitions`、`volumes`、`smart` | 30秒 |
| `windows_updates`、`event_logs` | 120秒 |

制限時間にはワーカープロセスの起動、収集、結果のシリアライズおよび終了に要する時間を含む。

引数なしの一括実行と`collect`では、次のオプションを繰り返し指定してコレクターごとの値を変更できる。

```text
--collector-timeout <collector>=<秒>
```

秒数は1から3,600の整数とする。同じコレクターは1回だけ指定できる。例:

```powershell
pcdiag.exe collect --output results `
  --collector-timeout smart=60 `
  --collector-timeout event_logs=300
```

## 状態と後続処理

コレクター全体が制限時間内に結果を確定できなかった場合は、次のように記録する。

- コレクターの`status`: `failed`
- `messages[].code`: `collector_timeout`
- `duration_ms`: 親プロセスがワーカー起動前から終了確認までに観測した時間
- 収集値: コレクター全体の失敗を表す`null`値
- 後続コレクター: 継続して実行

タイムアウトしたワーカーが途中まで作成した値は、プロセス間プロトコルで確定されていないため採用しない。期限内にコレクター自身が一部の値を確定して返した場合だけ`partial`とする。権限不足、非対応または実行条件を満たさず実行しなかった場合は`skipped`とし、タイムアウトには使用しない。

ワーカーの起動失敗、異常終了、プロトコル不正は`failed`と`collector_process_failed`へ変換し、可能な範囲で後続コレクターを継続する。すべての結果を統合できない内部プロトコル不整合だけは、不正な成果物を書き出さず収集全体を失敗させる。

## 中断処理との責務分担

タイムアウトは実行中の1コレクターだけを失敗として確定し、後続収集を継続する。`Ctrl+C`または`Ctrl+Break`は実行中のワーカーを終了したうえで収集全体を中断し、既存の中断規則に従って`.incomplete`と`interruption.log`を保持する。両者はワーカー終了処理を共有するが、後続処理の可否と成果物状態を分ける。

## Windows実機確認

1. Releaseビルドで通常の`collect`を実行し、13コレクターの結果と処理時間が`status.json`へ記録されることを確認する。
2. デバッガーまたは応答を停止できるテスト用Windows APIフックを使って任意のワーカーを停止し、そのコレクターの制限時間を1秒にする。
3. 対象が`failed`、理由が`collector_timeout`となり、直後のコレクターも実行されることを確認する。
4. タスクマネージャーまたはProcess Explorerで、停止したワーカーと子孫PowerShellが残っていないことを確認する。
5. 収集中に`Ctrl+C`を1回押し、実行中ワーカーが終了し、終了コード130、`.incomplete`、`interruption.log`が中断仕様に従うことを確認する。
