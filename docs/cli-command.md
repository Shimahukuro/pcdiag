# CLI基本仕様

## 引数なし実行

コマンドを省略した場合、次の処理を順番に実行する。

```text
pcdiag.exe
pcdiag.exe --output <出力先ディレクトリ>
```

`--output`を省略した場合は、現在の作業ディレクトリを出力ルートとする。指定した場合は、そのディレクトリを出力ルートとする。

```text
collect → diagnose → report
```

1. `collect`が新しいセッションディレクトリを作成する。
2. 作成したセッションディレクトリを入力として`diagnose`を実行する。
3. 同じセッションディレクトリを入力として`report`を実行する。
4. すべて成功した場合、標準出力へ`report`成果物ディレクトリのパスを出力する。

進捗は標準エラー出力へ表示する。いずれかの処理が失敗した場合は後続処理を実行せず、終了コード`1`で終了する。失敗前に完成した成果物は削除しない。

引数の解析に失敗した場合は終了コード`2`、`--help`およびすべての処理が成功した場合は終了コード`0`とする。

## Ctrl+Cによる中断

Windowsコンソールで`Ctrl+C`または`Ctrl+Break`を受けた場合、実行中の処理へ協調的な中断を要求する。コンソール制御ハンドラーは中断状態の更新だけを行い、ファイル操作やログ出力は通常の処理へ戻ってから行う。

最初の中断要求では、`collect`中の実行中コレクターワーカーを終了する。`diagnose`と`report`は次の中断確認点で処理を停止する。

- `collect`: 各コレクターの実行前後
- `diagnose`: 入力検証後、診断後、成果物の各書き込み前、成果物確定前
- `report`: 入力検証後、HTML生成後、成果物の各書き込み前、成果物確定前
- 引数なしの一括実行: 各工程の開始前

中断時は標準出力へ成果物パスを出力せず、標準エラーへ中断した工程と、存在する場合は`.incomplete`ディレクトリのパスを表示し、終了コード`130`で終了する。中断要求後は一括実行の後続工程を開始しない。

中断処理中に再度`Ctrl+C`または`Ctrl+Break`を受けた場合は、終了コード`130`で直ちに終了する。この場合は実行中の書き込み完了や中断ログの生成を保証しない。

### 中断時の成果物

- 中断前に正式名へ確定済みの成果物は保持し、変更または削除しない。
- 生成中の成果物は`.incomplete`のまま保持し、完成済み成果物として扱わない。
- `.incomplete`が作成済みの場合は、その直下へ`interruption.log`を保存する。ログには中断した工程と、成果物が未完成であることを記録する。
- `.incomplete`の作成前に中断した場合、または2回目の中断によって即時終了した場合は、標準エラーだけが中断の記録となることがある。
- 中断した成果物からの再開は行わない。残った`.incomplete`は利用者が確認してから削除する。

## 成果物の取り扱いに関する注意

引数なしの一括実行と、`collect`、`diagnose`および`report`の個別実行では、処理の開始時に成果物の取り扱いに関する注意を標準エラー出力へ一度表示する。一括実行では各工程ごとに繰り返さない。`--help`および引数解析に失敗して成果物を扱わない場合は表示しない。

標準出力は成功時の成果物パスだけを出力する既存の形式を維持する。表示内容と機微情報の取り扱い方針は、[`sensitive-data-policy.md`](sensitive-data-policy.md)を参照する。

## 個別実行

```text
pcdiag.exe collect --output <出力先ディレクトリ>
pcdiag.exe diagnose --output <セッションディレクトリ>
pcdiag.exe report --output <セッションディレクトリ>
```

個別実行の成果物形式と上書き防止規則は、引数なし実行でも変更しない。

## Windows Update履歴の収集オプション

Windows Update履歴は、既定で収集開始時刻から過去180日間、最大1,000件を新しい順に取得する。引数なしの一括実行と`collect`コマンドで、次のオプションを指定できる。

```text
--windows-update-days <日数|all>
--windows-update-max-entries <件数|all>
--windows-update-all
```

- `--windows-update-days`は1から3,650の日数、または期間制限を設けない`all`を受け付ける。
- `--windows-update-max-entries`は1から100,000の件数、または件数制限を設けない`all`を受け付ける。
- `--windows-update-all`は期間と件数の両方を無制限にする。
- 後に指定した個別オプションは`--windows-update-all`の設定を上書きできる。

例:

```text
pcdiag.exe collect --output results --windows-update-days 90 --windows-update-max-entries 500
pcdiag.exe collect --output results --windows-update-all
```

期間または件数により履歴を打ち切った場合は、`status.json`のWindows Updateコレクターへ切り捨て理由を記録する。

## コレクターのタイムアウト

引数なしの一括実行と`collect`では、`--collector-timeout <collector>=<秒>`を繰り返し指定して、コレクターごとの既定時間を上書きできる。秒数は1から3,600とし、同じコレクターを複数回指定した場合は引数エラーとする。

タイムアウトしたコレクターは`failed`、理由コード`collector_timeout`として`status.json`へ記録し、後続コレクターを継続する。実行方式、既定値、状態および実機確認方法は[`collector-timeouts.md`](collector-timeouts.md)を参照する。
