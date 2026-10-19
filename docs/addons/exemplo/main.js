// Addon de exemplo do D200 Deck: um contador e um interruptor de mentira.
//
// O app conversa com o addon por JSON, uma mensagem por linha: lê o que o
// addon escreve no stdout e escreve no stdin dele. O stdout é só do protocolo;
// mensagens de depuração vão para o stderr (console.error), que o app guarda
// no log (%APPDATA%\D200Deck\d200deck.log). Veja docs/addons.md.

'use strict';

const fs = require('node:fs');
const path = require('node:path');
const readline = require('node:readline');

/** As coisas que o interruptor liga e desliga (a lista do campo "item"). */
const ITEMS = [
  { value: 'luz', label: 'Luz da sala', short: 'Luz' },
  { value: 'som', label: 'Caixa de som', short: 'Som' },
  { value: 'ventilador', label: 'Ventilador' },
];

/** A contagem de cada tecla, por context, guardada na pasta de dados do addon. */
let counts = {};
let countsFile = null;
/** O que está ligado, por item. */
const switches = {};
/** As teclas do addon que podem aparecer agora. */
let keys = [];

function send(message) {
  process.stdout.write(JSON.stringify(message) + '\n');
}

function load() {
  try {
    counts = JSON.parse(fs.readFileSync(countsFile, 'utf8'));
  } catch {
    counts = {};
  }
}

function save() {
  fs.writeFileSync(countsFile, JSON.stringify(counts));
}

/** Manda ao app como uma tecla deve aparecer. */
function report(key) {
  if (key.action === 'count') {
    const n = counts[key.context] ?? 0;
    const unit = key.settings.unit || (Math.abs(n) === 1 ? 'toque' : 'toques');
    send({ type: 'state', context: key.context, label: `${n} ${unit}` });
  } else if (key.action === 'switch') {
    send({ type: 'state', context: key.context, on: switches[key.settings.item] ?? false, available: !!key.settings.item });
  }
}

function press(message) {
  if (message.action === 'count') {
    const step = Number(message.settings.step ?? 1) || 1;
    counts[message.context] = (counts[message.context] ?? 0) + (message.settings.down ? -step : step);
    save();
  } else if (message.action === 'switch') {
    const item = message.settings.item;
    if (!item) return 'escolha no editor o que ligar e desligar';
    switches[item] = !switches[item];
  } else {
    return `ação desconhecida: ${message.action}`;
  }
  return null;
}

function handle(message) {
  switch (message.event) {
    case 'hello':
      fs.mkdirSync(message.data_dir, { recursive: true });
      countsFile = path.join(message.data_dir, 'contagens.json');
      load();
      console.error(`Contador ${message.app_version ? `no D200 Deck ${message.app_version}` : ''} pronto`);
      break;
    case 'watch':
      keys = message.keys;
      keys.forEach(report);
      break;
    case 'press': {
      // Responder logo: o app espera a resposta (até 5 s) para saber se deu certo.
      const error = press(message);
      send(error ? { type: 'result', id: message.id, error } : { type: 'result', id: message.id });
      // Todas as teclas da mesma ação mostram o novo estado (duas teclas do mesmo item, por exemplo).
      keys.filter((key) => key.action === message.action).forEach(report);
      break;
    }
    case 'release':
      break;
    case 'options':
      if (message.field === 'item') send({ type: 'options', id: message.id, options: ITEMS });
      else send({ type: 'options', id: message.id, error: `campo sem lista: ${message.field}` });
      break;
    default:
      // Mensagens novas de versões futuras do app são ignoradas.
      break;
  }
}

readline.createInterface({ input: process.stdin }).on('line', (line) => {
  try {
    handle(JSON.parse(line));
  } catch (e) {
    console.error(`mensagem com problema: ${e.message}`);
  }
});
// Quando o app fecha o stdin, o readline termina e o addon sai sozinho.
