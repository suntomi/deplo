cli/src/command/vcs.rs の control_pr では　deplo vcs pr merge/close というpull requestに対して操作を行う２つのサブコマンドが実装されています。ここにdeplo vcs pr createというpull requestを作成するコマンドを追加してください。

PRの作成に必要なパラメーターは、例えばdeplo vcs pr merge のように -o "key=value" の形式で指定できるようにします。

```
merge pull request

Usage: cli vcs pr merge [OPTIONS] <url>

Arguments:
  <url>  URL of the pull request

Options:
  -o <option>      option for pull request merge.
                   -o $key=$value
                   for github, body options of https://docs.github.com/en/rest/pulls/pulls?apiVersion=2022-11-28#merge-a-pull-request can be specified.
                   plus, -o auto_merge=true to enable auto merge.
                   -o message=$text to post comment to pull request.
                   -o approve=true to approve the pull request.
                   TODO: for gitlab
  -h, --help       Print help
```
======
このコマンドラインツールは、いろいろなレポジトリで使われています。ツールが更新されたことをそういったユーザーのレポジトリ側で検出して更新のためのpull requestを作るようにしたいです。

私は、deplo initコマンドがスケジュールされたworkflowを起動するようなワークフロー設定ファイルを出力するようにして、その中で更新をチェックするのが良いと考えますが、あなたはどのようなやり方が良いと考えますか？

======
cli/src/command/vcs.rs の control_pr にさらに deplo vcs pr search というコマンドを追加します。レポジトリの issues APIを使い、githubの実装ではpull requestでないものを除いたレスポンスを返します。

eg) `GET /repos/{owner}/{repo}/issues?state=open&labels=bug,frontend`

クエリパラメーターはコマンドラインオプションから -f "key=value" という形で与えます。

インターフェイスを統一するため、 core/src/vcs.rs のVCSトレイトには search_pr(filters: &Vec<String>) のような関数を追加します。

filtersは、上記のコマンドラインオプションの "key=value" の値が配列としてそのまま渡されるようにしてください。

======
次に deplo vcs label create というコマンドを実装します。 cli/src/command/vcs.rs に control_label という関数を実装し、labelというサブコマンドでcontrol_labelが呼ばれるようにします。control_labelの実装は今の所createのみでいいです。

インターフェイスを統一するため、 core/src/vcs.rs のVCSトレイトには label(name: &str, color: Option<&str>) のような関数を追加します。
githubは `POST https://api.github.com/repos/OWNER/REPO/labels` APIを使います。
gitlab側はエラーを返すようにしておけば良いです。

全体的なコマンドは

```
deplo vcs label create $label_name --color (or -c) #FFFF00
```

のようにしてください。

======
以下の2点を追加で実装してください。

- デフォルトでは最新版のバージョン名が "[0-9]+\.[0-9]+\.[0-9]+" というパターンのみを更新対象にする。 eg. 0.5.0-betaや0.6.0-rc1 といった正式でないリリースに対してPRを作成することで不安定なバージョンを導入しないように。更新対象にするパターンは、action vars経由で指定する正規表現で変更できる
- PRを作成しようとした時に、すでに更新用のPRが作成されている場合、それをcloseするようにします。更新用のPRにはdeplo-updateという名称のラベルをつけて、そのラベルのついたopenなPRがないかチェックするのが良いと考えています。

======
今の実装ですが、気になる点があります。deploのバージョン更新のpull requestがマージされないまま次のworkflowが実行されると、pull requestがcloseされて、新たに同じ内容のPRが作成され続けませんか？それはあまり良くないため、同じバージョン向けの更新用pull requestが作られていたら、closeしないようにしたいです。どのように実装するかアイデアがあれば教えてください。

======
今、deploが処理するレポジトリのdeplo-main.yamlは、Deplo.tomlの`checkout` という設定を見てcheckoutのやり方を切り替えています。
現在deplo-update.yamlのcheckoutはそれを見れていないようです。deplo-update.yamlについてもcheckout周りのステップを生成するのに generate_checkout_stepsを使った方が良いのではないでしょうか？