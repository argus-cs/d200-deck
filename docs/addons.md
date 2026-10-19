# Addons

Addons trazem ações novas para as teclas do D200 Deck: jogos, apps e serviços que o app não conhece. Cada addon é um programa, feito em qualquer linguagem, que o app abre e com quem conversa por mensagens de texto. O addon recebe os toques nas teclas dele e pode mudar o que elas mostram.

## Para quem usa

### Instalar

1. Abra a página **Addons** no D200 Deck e clique em **Abrir a pasta de addons** (`%APPDATA%\D200Deck\addons`).
2. Copie para lá a pasta do addon. Cada addon fica numa pasta própria, com um `addon.json` dentro.
3. Volte à página **Addons**: ela procura de novo ao abrir, ou clique em **Procurar de novo**.
4. Ligue o addon na chave dele. Na primeira vez, o app mostra quem fez o addon e o que ele vai rodar, e pede confirmação.

Um addon novo chega sempre desligado.

### O addon da Steam

O D200 Deck já vem com um addon: o **Steam**. Ele começa ligado se a Steam estiver instalada no PC; depois, vale o que você escolher na página **Addons**. Ele tem duas ações:

- **Abrir jogo**: escolha o jogo numa lista dos jogos instalados, e a tecla ganha a capa e o nome dele. A tecla acende enquanto o jogo está aberto e escurece se ele for desinstalado. Com o jogo já aberto, um toque traz a janela dele para a frente (ou não faz nada, se você escolher).
- **Abrir a Steam**: abre a biblioteca, os amigos, os downloads, a loja ou as configurações da Steam.

### Usar numa tecla

No editor da tecla, em **Ação**, as ações dos addons ligados ficam no grupo **Addons** (como "Contador › Contar"). Os campos de cada ação aparecem logo abaixo, e as listas que o addon preenche (os jogos instalados, por exemplo) têm um botão para atualizar.

O visual da tecla continua sendo seu: ícone, texto, cores e segundo estado se mudam como em qualquer tecla. O addon só sugere um texto e um ícone quando você escolhe a ação ou um item de uma lista, sem apagar o que você já tinha escolhido. Se a ação muda o texto da tecla sozinha (um contador), a opção **Deixar o addon mudar o texto da tecla** decide se esse texto aparece ou se fica sempre o seu.

### Quando algo dá errado

- Com o addon desligado, parado ou removido, as teclas dele ficam escuras e não fazem nada. Elas guardam o que foi configurado e voltam a funcionar quando o addon voltar.
- Um addon que fecha sozinho é aberto de novo, esperando um pouco mais a cada vez. Se ele fechar 5 vezes seguidas, o app desiste, e a página **Addons** mostra a última mensagem dele, com **Tentar de novo** e **Abrir o log**.
- Tudo o que o addon escreve no log fica em `%APPDATA%\D200Deck\d200deck.log`, com o id dele na frente.

### Segurança

Um addon é um programa como outro qualquer: roda com as suas permissões do Windows e pode ler e mudar os seus arquivos, acessar a internet e apertar teclas. Instale só addons de quem você confia. O D200 Deck não limita o que o addon faz. O que ele garante:

- O addon só roda depois que você liga.
- A janela do app nunca mostra conteúdo do addon: os campos são desenhados pelo próprio app a partir do `addon.json`.
- As imagens que o addon sugere só são aceitas de dentro da pasta dele ou da pasta de dados dele.
- Quando o D200 Deck fecha, ou mesmo se ele cair, o Windows encerra os addons junto.

## Para quem faz um addon

### Comece pelo exemplo

[`docs/addons/exemplo`](addons/exemplo) é um addon completo em Node.js, sem dependências: uma tecla que conta toques e mostra o número, e um interruptor que mostra se está ligado, com uma lista preenchida pelo addon. Para experimentar, copie a pasta para `%APPDATA%\D200Deck\addons`, ligue o "Contador" na página **Addons** e escolha "Contador › Contar" numa tecla. É preciso ter o Node.js no PATH.

### O `addon.json`

```json
{
  "id": "exemplo.contador",
  "name": "Contador",
  "version": "0.1.0",
  "author": "Seu nome",
  "description": "O que o addon faz, numa frase.",
  "api": 1,
  "run": ["node", "main.js"],
  "actions": [
    {
      "id": "count",
      "name": "Contar",
      "description": "Aparece no editor, abaixo da ação.",
      "icon": "plus",
      "label": "Contar",
      "state": false,
      "dynamic": true,
      "fields": [{ "id": "step", "type": "number", "label": "Quanto somar", "default": 1, "min": 1 }]
    }
  ]
}
```

| Campo | O que é |
|---|---|
| `id` | Identifica o addon nas teclas e na config: letras minúsculas e números, separados por pontos ou traços (`fulano.steam`). Não mude depois de publicar, ou as teclas perdem o addon. |
| `name`, `version`, `author`, `description` | O que a página **Addons** mostra. |
| `api` | A versão do protocolo para a qual o addon foi feito. Hoje é `1`. Um app mais velho recusa addons de uma versão que ele não entende. |
| `run` | O programa e os argumentos. Um caminho com barra (`bin/addon.exe`) é relativo à pasta do addon; um nome sozinho (`node`, `python`) é procurado primeiro na pasta do addon e depois no PATH. |
| `actions` | As ações que o addon oferece para as teclas. |

Cada ação:

| Campo | O que é |
|---|---|
| `id`, `name` | O id (letras, números, `_` e `-`) e o nome mostrado no editor. |
| `description` | Opcional, mostrada no editor. |
| `icon`, `label` | O visual sugerido para a tecla. O `icon` é o nome de um ícone embutido do app (`play`, `gamepad`, `zap`…) ou um `.png`/`.svg` dentro da pasta do addon. |
| `state` | `true` quando o addon diz se a tecla está ligada ou desligada. Desligada, a tecla mostra o segundo estado ou, sem ele, escurece. |
| `dynamic` | `true` quando o addon muda o texto da tecla. |
| `fields` | Os campos que a pessoa preenche no editor. Os valores ficam na config e chegam ao addon em `settings`. |

Cada campo tem `id`, `type` e `label`, e opcionalmente `hint` (uma explicação abaixo do campo), `placeholder` e `default`. Os tipos:

- `text`: um texto.
- `number`: um número, com `min` e `max` opcionais.
- `checkbox`: sim ou não.
- `select`: uma lista. As opções vêm em `options` (`[{ "value": "a", "label": "Opção A" }]`) ou, com `"dynamic": true`, o app pede ao addon enquanto ele roda.

Uma opção de lista pode sugerir o visual da tecla: `short` é um texto curto para a tecla e `icon` uma imagem, que é um ícone embutido, um arquivo da pasta do addon ou um caminho completo dentro da pasta de dados dele.

### Como o app roda o addon

- O programa roda na pasta do addon, sem janela de console. O stdin e o stdout são do protocolo; tudo o que ele escreve no **stderr** vai para o log do app.
- Ele é aberto quando a pessoa liga o addon (ou quando o app abre, se já estava ligado) e encerrado quando ela desliga, sem aviso. Guarde o que precisar enquanto roda.
- Cada addon tem uma pasta de dados própria, `%APPDATA%\D200Deck\addon-data\<id>`, que chega na mensagem `hello`.
- Se o programa fechar, o app abre de novo depois de 1, 2, 4 e 8 segundos. Na quinta vez seguida (cada uma com menos de 30 s rodando), ele desiste e mostra a última linha do stderr.
- Programas que o addon abre não fazem parte dele: continuam abertos quando o addon ou o app fecham.
- Mudou o `addon.json`? **Procurar de novo** na página **Addons** reabre o addon com o manifesto novo.
- Os addons que vêm com o app ficam na pasta de instalação (`addons`). Um addon seu com o mesmo `id` toma o lugar do que veio com o app, o que serve para testar uma versão nova. Só eles podem usar `$app` no `run`, que é o próprio `d200-deck.exe`: o da Steam roda como `d200-deck.exe --addon steam`.

### O protocolo

Cada mensagem é um objeto JSON numa linha, em UTF-8. O app escreve no stdin do addon e lê o stdout dele. Mensagens desconhecidas e campos a mais devem ser ignorados dos dois lados: é assim que versões novas acrescentam coisas sem quebrar os addons de antes. Uma linha de mais de 1 MiB é descartada.

Cada tecla do addon tem um `context`: um texto estável, igual para todas as teclas com a mesma ação e as mesmas configurações. É por ele que o addon diz como as teclas aparecem, e ele serve para guardar dados por tecla (o exemplo guarda a contagem assim).

#### Do app para o addon (campo `event`)

`hello` chega primeiro, logo depois que o addon abre:

```json
{"event":"hello","api":1,"app_version":"0.4.0","addon_dir":"C:\\…\\addons\\contador","data_dir":"C:\\…\\addon-data\\exemplo.contador","language":"pt-BR"}
```

`watch` traz todas as teclas do addon que podem aparecer agora: as do layout e das regras valendo, as das pastas e as duas faces das teclas de dois estados. A lista vem inteira a cada mudança, e vazia quando nenhuma aparece. Mande o estado de cada uma ao receber.

```json
{"event":"watch","keys":[{"context":"count-1f0c…","action":"count","settings":{"step":2}}]}
```

`press` é um toque. Responda com `result` e o mesmo `id` em até 5 segundos; sem resposta, o app mostra um erro. Faça a parte demorada depois de responder.

```json
{"event":"press","id":12,"context":"count-1f0c…","action":"count","settings":{"step":2}}
```

`release` é a tecla sendo solta, depois de um `press` (para ações como "falar enquanto segura"). Não tem resposta.

`options` pede as opções de um campo com `"dynamic": true`, quando o editor mostra a lista. Responda com `options` e o mesmo `id` em até 5 segundos.

```json
{"event":"options","id":13,"action":"switch","field":"item","settings":{}}
```

#### Do addon para o app (campo `type`)

`state` diz como as teclas de um `context` aparecem. Cada mensagem substitui a anterior inteira. Os campos que faltam ficam com o valor padrão:

```json
{"type":"state","context":"count-1f0c…","label":"12 toques","on":true,"available":true}
```

- `available`: `false` escurece a tecla (o jogo não está instalado, o serviço está fora do ar). Padrão: `true`.
- `on`: para ações com `state`. `false` mostra o segundo estado da tecla ou a escurece.
- `label`: para ações com `dynamic`, o texto no lugar do da tecla, se a pessoa deixar.

`result` responde ao `press`; com `error`, o texto aparece na página **Ao vivo**:

```json
{"type":"result","id":12}
{"type":"result","id":12,"error":"o OBS não está aberto"}
```

`options` responde ao pedido de opções, com a lista ou com um erro:

```json
{"type":"options","id":13,"options":[{"value":"luz","label":"Luz da sala","short":"Luz"}]}
{"type":"options","id":13,"error":"faça login primeiro"}
```

`log` escreve no log do app. O `level` é `debug`, `info` (o padrão), `warn` ou `error`:

```json
{"type":"log","level":"warn","message":"a conexão caiu, tentando de novo"}
```

### Um addon mínimo em Python

```python
import json, sys

def send(message):
    print(json.dumps(message), flush=True)

for line in sys.stdin:
    message = json.loads(line)
    if message["event"] == "press":
        print(f"apertou {message['action']}", file=sys.stderr, flush=True)
        send({"type": "result", "id": message["id"]})
```

Com `"run": ["python", "main.py"]` no `addon.json`. O `flush=True` importa: sem ele, o Python guarda a saída e o app só recebe a resposta muito depois.
