// your-turn: a line under the prompt when Claude waits for you.
//
// A turn can end with Claude waiting for the user, or waiting for its own
// background work (a build, a Monitor, a subagent, a scheduled wakeup) to
// wake it. The prompt looks the same in both; this tells them apart.
//
// - turn.start: clears the line and notes whether a typed prompt started
//   the turn or a background notification did.
// - turn.complete (main loop): reads what yhooky's turn-state check wrote at
//   Stop (<state dir>/<session>.stop.json: background tasks that can still
//   wake the session, resident dev servers left out, and scheduled wakeups).
//   Nothing left to wake it: the user's turn. Otherwise a small model reads
//   the turn's last message and says USER or WAIT.
// - Each decision is appended to <session>.verdicts.jsonl, to check it
//   afterwards.
//
// State dir: $TURN_STATE_DIR, else ~/.claude/state/turn-state (as yhooky).
//
// TODO: a background subagent that stops to ask for input (Notification
// `agent_needs_input`) does not show the line yet: classic Notification
// hooks do not reach a mod, so yhooky would have to record it and this
// module read it.

import type { EngineInterface as Engine, Register } from 'claude-code'

const MODEL = 'haiku'
const MAX_TASKS = 10
const MAX_TASK_CHARS = 150
const MAX_ANSWER_CHARS = 2000

// Measured on 120 past turn ends (80 to tune, 40 held out): 97.5% on the
// held-out set with the background list and the turn's start included.
const SYSTEM = `You decide whether an AI coding assistant, at the end of its turn, is now waiting for the human user (USER) or will resume by itself when background work finishes (WAIT).

You are given how the turn started (a message typed by the user, or a notification from a background process), the background processes still running, and the assistant's final message of the turn.

Answer USER when the final message:
- asks the user a question, or asks them to decide, approve, confirm, choose or reply;
- asks the user to do, test, check, look at or send something now;
- finishes the requested work without saying the assistant will continue on its own.

Answer WAIT when the final message says or implies that the assistant will continue by itself after background work, and asks nothing of the user. Typical signs: 「待つ」「待機」「まだ走っている」「まだ動いている」「終わったら」「届いたら」「結果が出たら」「完了後」, "waiting for", "will report when".
Also answer WAIT for a routine update inside a loop driven by background events: the turn started from a background notification and the message only reports what the assistant just updated or recorded (for example notes appended while a long-running job streams its output), unless it asks the user something.

Background processes that simply keep running (dev servers, watchers) do not by themselves mean WAIT.
If the message both waits for background work and asks the user something, answer USER.

Reply with exactly one word: USER or WAIT.`

type Task = { id: string; type: string; description: string; command: string }
type StopState = { at: number; tasks: Task[]; dropped: Task[]; crons: number }
type Started = 'user' | 'notification'

async function stateDir($: Engine): Promise<string | undefined> {
  const dir = await $.env.get('TURN_STATE_DIR')
  if (dir) return dir
  const home = (await $.env.get('USERPROFILE')) ?? (await $.env.get('HOME'))
  return home === undefined ? undefined : `${home}/.claude/state/turn-state`
}

function stem(session: string): string {
  return session.replace(/[^A-Za-z0-9_-]/g, '_')
}

async function readStop($: Engine, path: string): Promise<StopState | undefined> {
  try {
    return JSON.parse(String(await $.fs.read(path))) as StopState
  } catch {
    return undefined
  }
}

function promptOf(answer: string, stop: StopState, started: Started): string {
  const names = stop.tasks.map(t =>
    `${t.type}: ${t.command || t.description}`.replace(/\s+/g, ' ').trim(),
  )
  const uniq = [...new Set(names)]
  const shown = uniq.slice(0, MAX_TASKS).map(n => `- ${n.slice(0, MAX_TASK_CHARS)}`)
  if (uniq.length > MAX_TASKS) shown.push(`- (${uniq.length - MAX_TASKS} more)`)
  if (stop.crons > 0) shown.push(`- ${stop.crons} scheduled wakeup(s)`)
  const from =
    started === 'notification' ? 'a notification from a background process' : 'a message typed by the user'
  return `The turn started from: ${from}\n\nBackground processes still running:\n${shown.join('\n')}\n\nThe assistant's final message of this turn:\n<<<\n${answer.slice(-MAX_ANSWER_CHARS)}\n>>>`
}

// The turn in progress; the module's own, so a reload starts them over.
let turnId: string | undefined
let startedAt = 0
let started: Started = 'user'
let promptHead = ''

const MAX_VERDICTS = 200

// Appends the decision to the session's verdict log, keeping the latest
// MAX_VERDICTS lines.
async function record($: Engine, file: string | undefined, verdict: Record<string, unknown>): Promise<void> {
  if (file === undefined) return
  const at = await $.clock.now()
  const old = await $.fs.read(file).then(String, () => '')
  const lines = [...old.split('\n').filter(l => l.trim()), JSON.stringify({ at, ...verdict })]
  await $.fs.write(file, lines.slice(-MAX_VERDICTS).join('\n') + '\n').catch(() => undefined)
}

// When the latest turn ended, and the timer redrawing its line.
let endedAt = 0
let ticker: { cancel: () => void } | undefined

type Kind = 'user' | 'error' | 'refusal'

// `2 min`, `1h 15m`; nothing under a minute.
function duration(ms: number): string | undefined {
  const minutes = Math.floor(ms / 60_000)
  if (minutes < 1) return undefined
  const hours = Math.floor(minutes / 60)
  if (hours === 0) return `${minutes} min`
  return minutes % 60 === 0 ? `${hours}h` : `${hours}h ${minutes % 60}m`
}

// The status text, read after the `⚠your-turn:` the engine puts first
// (`⚠your-turn: for 2 min`).
function line(kind: Kind, bg: number, now: number): string {
  const t = duration(now - endedAt)
  const head =
    kind === 'user'
      ? t === undefined ? 'just now' : `for ${t}`
      : `${kind === 'error' ? 'stopped on an API error' : 'stopped on a refusal'}${t === undefined ? '' : ` · ${t} ago`}`
  return bg > 0 ? `${head} · ${bg} task${bg === 1 ? '' : 's'} in bg` : head
}

// Shows the line while `id` is still the latest turn, with the time since
// that turn ended, redrawn every minute until the next turn starts.
async function show($: Engine, id: string, kind: Kind, bg = 0): Promise<void> {
  const now = await $.clock.now()
  if (turnId !== id) return
  $.ui.status(line(kind, bg, now))
  ticker?.cancel()
  ticker = $.clock.every(60_000, () => {
    if (turnId !== id) {
      ticker?.cancel()
      return
    }
    void $.clock.now().then(t => {
      if (turnId === id) $.ui.status(line(kind, bg, t))
    })
  })
}

async function judge($: Engine, id: string, reason: string, answer: string): Promise<void> {
  const dir = await stateDir($)
  const session = stem(await $.session.id())
  const file = dir === undefined ? undefined : `${dir}/${session}.verdicts.jsonl`
  const base = { turnId: id, started, promptHead, reason }

  if (reason === 'aborted') return record($, file, { ...base, decision: 'none' })
  if (reason === 'error') {
    await show($, id, 'error')
    return record($, file, { ...base, decision: 'error' })
  }
  if (reason === 'refusal') {
    await show($, id, 'refusal')
    return record($, file, { ...base, decision: 'refusal' })
  }
  const stop = dir === undefined ? undefined : await readStop($, `${dir}/${session}.stop.json`)
  // No record from this turn's Stop (yhooky missing or failed): nothing
  // shows Claude will resume by itself, so it is the user's turn.
  if (stop === undefined || stop.at < startedAt) {
    await show($, id, 'user')
    return record($, file, { ...base, decision: 'user', stop: 'missing' })
  }
  if (stop.tasks.length === 0 && stop.crons === 0) {
    await show($, id, 'user')
    return record($, file, { ...base, decision: 'user', tasks: 0, dropped: stop.dropped.length })
  }
  const r = await $.model.complete({
    model: MODEL,
    system: SYSTEM,
    prompt: promptOf(answer, stop, started),
    maxTokens: 5,
  })
  const label = !r.isAnswered ? 'ERROR' : /^\s*WAIT/i.test(r.text) ? 'WAIT' : 'USER'
  const extra = { tasks: stop.tasks.length, crons: stop.crons, label, usage: r.usage }
  if (label === 'WAIT') return record($, file, { ...base, decision: 'wait', ...extra })
  // A failed call counts as the user's turn: a line too many beats a miss.
  await show($, id, 'user', stop.tasks.length + stop.crons)
  return record($, file, { ...base, decision: 'user', ...extra })
}

type Verdict = { at: number; turnId: string; decision: string; tasks?: number; crons?: number }

// On load (session start, resume, /reload-plugins), redraws the line from
// the session's last verdict: a reload keeps the line the old module drew,
// in its old wording and with its timer stopped.
async function restore($: Engine): Promise<void> {
  const dir = await stateDir($)
  if (dir === undefined) return
  const session = stem(await $.session.id())
  const log = await $.fs.read(`${dir}/${session}.verdicts.jsonl`).then(String, () => '')
  const last = log.trim().split('\n').pop()
  if (turnId !== undefined) return // a turn began meanwhile; it draws its own
  let v: Verdict | undefined
  try {
    v = last ? (JSON.parse(last) as Verdict) : undefined
  } catch {
    v = undefined
  }
  const kind: Kind | undefined =
    v?.decision === 'user' ? 'user' : v?.decision === 'error' ? 'error' : v?.decision === 'refusal' ? 'refusal' : undefined
  if (v === undefined || kind === undefined) {
    $.ui.status(undefined)
    return
  }
  turnId = v.turnId
  endedAt = v.at
  await show($, v.turnId, kind, (v.tasks ?? 0) + (v.crons ?? 0))
}

export const register: Register = on => {
  on('session.start', async ($, e, next) => {
    const done = await next(e)
    void restore($).catch(() => undefined)
    return done
  })

  on('turn.start', async ($, e, next) => {
    turnId = e.turnId
    startedAt = await $.clock.now()
    const text = e.text.trimStart()
    started = text === '' || text.startsWith('<task-notification>') ? 'notification' : 'user'
    promptHead = text.slice(0, 20)
    ticker?.cancel()
    ticker = undefined
    $.ui.status(undefined)
    return next(e)
  })

  on('turn.complete', async ($, e, next) => {
    const done = await next(e)
    endedAt = await $.clock.now()
    // Nobody sees the line where nothing draws (a plain -p run): skip the
    // model call there, unless YOUR_TURN_HEADLESS is set to test the mod.
    const isDrawn = (await $.session.surfaces()).length > 0 || Boolean(await $.env.get('YOUR_TURN_HEADLESS'))
    if (isDrawn && e.agentId === undefined && e.turnId === turnId) {
      // Not awaited: the model's answer must not hold up the prompt.
      void judge($, e.turnId, e.reason, e.answer).catch(() => undefined)
    }
    return done
  })
}
