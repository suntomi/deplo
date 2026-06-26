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

======
core::config::Module::ci_by_envですが、これはCIの環境上でdeploが動いている場合に、対応するciの設定を自動で取得するためのものです。しかし、CIの環境上で動いていない場合にはdefaultのci設定を返しています。

このことは手元でdeploのコマンドラインを動かす場合に以下のような問題を起こしています。
- ローカルではdefaultのci設定でしか起動できない
- そしてそのことに気づきづらい

このため以下のようにします。
- deploの最上位のコマンドライン引数に`--ci=$ci_name` を用意する
- $ci_nameはDeplo.tomlのci tableのキー
- core::config::Module::ci_by_env で、対応するCI環境が見つからなかった場合、`--ci=$ci_name`で与えられた指定に対応するciを使う。
  - このケースで`--ci`が与えられていない場合、warningのログを出して、defaultのci設定を返す

======

- core/src/ci/ghaction.rs の GhAction::generate_configの実装ですが、設定ファイルはDeplo.tomlでそのCI上で動作するjobがない場合作成されません。従って core/src/ci/ghaction.rs:876あたりからのsecret/variableを設定する部分がそのciにおいて常に実行されてしまいます。これらのコードを if jobs.len() == 0 の後に移動させてください。
- generate_update_workflow ですが、update workflowは１レポジトリで１つあれば良いのですが、現状では複数のアカウントがあるとその数だけ作成されてしまいます。generate_update_workflow を生成するのはdefaultのci accountの時だけにしてください。

======
--ciがないコマンドでwarningを出す件ですが、ログレベルをdebugにします。コマンドの中には出力をshellで利用するものがあり(eg. deplo job output)、warningだと必ずその出力に混じってしまうからです。どのコマンドが出力を利用するものか、は機械的に判定が難しいので、デフォルトでは--ciが指定

=======
CIからdeploが起動されるときに、利用すべきciの設定がどれになるかわかるように、core/src/ci/ghaction.rs の generate_config で生成されるワークフローファイル(eg. .github/workflows/deplo-main.yml)に新しい環境変数を追加します。

core/res/ci/ghaction/common_envs.yml.tmpl に DEPLO_CI_ACCOUNT_NAME を追加して generate_config を行うciのaccout_nameで初期化します。

GhAction::runs_on_service, CircleCI::runs_on_serviceは 既存のコードを置き換え、ciのaccount_nameが DEPLO_CI_ACCOUNT_NAME と一致するか、という判定に変更してください。

=======
core/src/config/ci.rs の Accountsにci_typeという&strを返す関数を追加します。
GhAction/GhActionAppは"GhAction", CircleCIは"CircleCI"を返します。

Accounts::type_as_str()は以下の３カ所で呼ばれています
core/src/config.rs
core/src/ci/ghaction.rs
core/src/config/ci.rs

それぞれ以下のように修正します。
core/src/config.rs => ci_type() に変更
core/src/ci/ghaction.rs => 呼び出すのをやめ、ci_typeを "GhAction" 固定にする
core/src/config/ci.rs => ci_type() に変更し、呼び出している関数名を type_matched => ci_type_matched に変更

付随してci::Accounts::is_mainを以下のように修正します
- typesではなく単一のtype(&str)を受け取るようにする
- circleci側はis_main("CircleCI), ghaction側はis_main("GhAction")と変更

=======
core/res/ci/ghaction/update.yml.tmpl の `Create update pull request` step でpull requestを作成していますが、PRの本文に差分を表示するためのリンクを含むようにしてください。  `$DEPLO_UPDATE_CURRENT_VERSION` と `$latest` の差分が見れれば良いです。

=======
cli/src/command/ci.rs ですが、現在 deplo ci setenvで全てのactions vars/secretsを設定しますが、以下のように個別のvars/secretsを設定/取得できるようにします。

$keyという名前の単独のsecretを設定する: deplo ci secret $key $value 
$keyという名前の単独のvarを設定する: deplo ci var $key $value 
$keyという名前の単独のsecretの値を得る: deplo ci secret $key 
$keyという名前の単独のvarの値を得る: deplo ci var $key

=======
削除までサポートしたいので、値を設定するときの構文を以下のように変えます
$keyという名前の単独のsecretを設定する: deplo ci secret $key=$value 
$keyという名前の単独のvarを設定する: deplo ci var $key=$value 
$keyには=が含まれないとして良いため、左から最初に現れた=の場所で区切ってください。
=以降が空文字列だった場合は削除することとします。

従って、ci::CI::set_var, set_secretについて、値が空文字列の場合はvar/secretを削除するように修正してください。
=======
github action secrets/variable ですが、以下の仕様変更があります。
- secretを設定できるカテゴリーにagents, codespacesが追加されています。このサポートを追加してください。
- variableを設定できるカテゴリーにagentsが追加されています。このサポートを追加してください。
  - このため、Deplo.tomlのvarsもsecretsと同様にtargetsを持つ必要があります。また、G_SECRET_TARGETSに倣って、G_VARS_TARGETSのようなものを作る必要もあります。

修正後、targetsが設定されていない場合のtargetのデフォルトは以下のようにします。
- secret => actions, dependabot (変化なし)
  - 今デフォルト値としてG_ALL_SECRET_TARGETSが使われていますがG_DEFAULT_SECRET_TARGETSとします。
- var => actions (変化なし)
  - デフォルト値としてG_DEFAULT_VAR_TARGETSを用意して利用するようにしてください。