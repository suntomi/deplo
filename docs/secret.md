# about secret management

secretは

- env or fileから取得する(local)
- 何らかのsecret managerから取得する(remote)

local typeはdeplo setenvでCI側にアップロードしておきCIではすべて環境変数として参照する
remote typeはlocalでもCIでも同じように、何らかの方法で認証を行う

deplo setenv => local typeのsecretをCIに移動させる


remote typeは一方で、localでどのように実行させるかが問題
おそらく、suntomiドメインのid providerを立ち上げておいて、そこ経由でassume role的なことをできるように設定してもらうことになるだろう

https://github.com/ramosbugs/openidconnect-rs とかを使い、oidc provider機能を実装する.
.
