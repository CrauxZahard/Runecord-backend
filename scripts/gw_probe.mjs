// End-to-end probe for /gateway/v1: HELLO, IDENTIFY, READY, HEARTBEAT, RESUME, replay.
// Usage: node scripts/gw_probe.mjs <bot-token> <guild-id> <channel-id>
const token = process.argv[2];
const guild = process.argv[3] || "100";
const channel = process.argv[4] || "211";
const base = "http://127.0.0.1:47800";
const wsUrl = "ws://127.0.0.1:47800/gateway/v1";
const intents = (1 << 0) | (1 << 1) | (1 << 7) | (1 << 8) | (1 << 9) | (1 << 12) | (1 << 15);

function client() {
  const ws = new WebSocket(wsUrl);
  const queue = [];
  const waiters = [];
  ws.addEventListener("message", (ev) => {
    const msg = JSON.parse(ev.data);
    queue.push(msg);
    for (let i = waiters.length - 1; i >= 0; i--) {
      if (waiters[i].pred(msg)) {
        waiters[i].resolve(msg);
        waiters.splice(i, 1);
      }
    }
  });
  const waitFor = (pred, ms = 6000) =>
    new Promise((resolve, reject) => {
      const hit = queue.find(pred);
      if (hit) return resolve(hit);
      const timer = setTimeout(() => reject(new Error("timeout")), ms);
      waiters.push({ pred, resolve: (m) => { clearTimeout(timer); resolve(m); } });
    });
  const open = new Promise((resolve, reject) => {
    ws.addEventListener("open", resolve, { once: true });
    ws.addEventListener("error", reject, { once: true });
  });
  return { ws, open, waitFor };
}

async function post(content) {
  const res = await fetch(`${base}/api/v1/channels/${channel}/messages`, {
    method: "POST",
    headers: { Authorization: `Bot ${token}`, "Content-Type": "application/json" },
    body: JSON.stringify({ content }),
  });
  if (!res.ok) throw new Error(`send failed ${res.status}: ${await res.text()}`);
  return res.json();
}

const first = client();
await first.open;
const hello = await first.waitFor((m) => m.op === 10);
console.log("HELLO", hello.d);

first.ws.send(JSON.stringify({ op: 2, d: { token: `Bot ${token}`, intents } }));
const ready = await first.waitFor((m) => m.t === "READY");
const sessionId = ready.d.session_id;
let lastSeq = ready.s;
console.log("READY", { session: sessionId, seq: lastSeq, guilds: ready.d.guilds.length });

first.ws.send(JSON.stringify({ op: 1, d: lastSeq }));
const ack = await first.waitFor((m) => m.op === 11);
console.log("HEARTBEAT_ACK", ack.op);

const sent = await post("probe: online dispatch");
const live = await first.waitFor((m) => m.t === "MESSAGE_CREATE" && m.d.content === "probe: online dispatch");
lastSeq = live.s;
console.log("LIVE_DISPATCH", { seq: live.s, id: live.d.id, author: live.d.author.username });

first.ws.close();

const offline = await post("probe: sent while offline");
console.log("OFFLINE_SEND", offline.id);

const second = client();
await second.open;
await second.waitFor((m) => m.op === 10);
second.ws.send(JSON.stringify({ op: 6, d: { token: `Bot ${token}`, session_id: sessionId, seq: lastSeq } }));
const resumed = await second.waitFor((m) => m.t === "RESUMED");
const replayed = await second.waitFor((m) => m.t === "MESSAGE_CREATE" && m.d.content === "probe: sent while offline");
console.log("RESUMED", { seq: resumed.s, replayedSeq: replayed.s, id: replayed.d.id });

second.ws.close();
console.log("OK");
process.exit(0);
