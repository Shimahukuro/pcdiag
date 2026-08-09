# 更新確認

pcdiagは通常の起動時に、固定された公式GitHubリポジトリのReleases APIへHTTPSのGETリクエストを最大1回送信する。

```text
https://api.github.com/repos/Shimahukuro/pcdiag/releases?per_page=100
```

リクエストにはpcdiagのバージョンを含む`User-Agent`とGitHub API用のヘッダーが含まれる。診断成果物、端末名、ユーザー名、デバイス情報などを読み取ったり送信したりしない。通常のHTTPS通信と同様、接続元IPアドレスなどの通信情報は接続先から確認できる。

接続タイムアウトは1秒、リクエスト全体のタイムアウトは2秒とする。リダイレクトには追従しない。オフライン、タイムアウト、レート制限、HTTPエラー、JSON不正などで確認できなかった場合は通知せず、要求された診断処理と本来の終了コードへ影響させない。

## Releaseの選択

GitHub上でdraftではない公開済みReleaseを候補とする。現在のRelease運用では通常のバージョンもGitHub上のprereleaseとして公開しているため、GitHub APIの`prerelease`属性による除外は行わない。

Releaseタグの先頭にある`v`を除き、Semantic Versioningとして解釈できる候補の最大値を選ぶ。文字列比較は行わない。例えば`0.4.0-alpha.2`は`0.4.0-alpha.1`より新しく、安定版の`0.4.0`はどちらよりも新しい。不正なタグは無視する。

選択したバージョンが実行中の`CARGO_PKG_VERSION`より新しい場合だけ、現在のバージョン、最新バージョン、および次の固定URLを標準エラー出力へ表示する。

```text
https://github.com/Shimahukuro/pcdiag/releases
```

APIレスポンスに含まれるURLを開いたり実行したりすることはない。自動ダウンロードおよび自動更新も行わない。

## 無効化

外部通信を行わない場合は`--no-update-check`を指定する。

```powershell
pcdiag.exe --no-update-check
pcdiag.exe diagnose --output <セッションディレクトリ> --no-update-check
```

`--help`、引数エラーおよび内部コレクタープロセスでも更新確認は行わない。
