import { useEffect, useState } from 'react';
import { invoke } from '../api/tauri';
import { timerRemaining, type StageState, type TimerAction } from '../api/useEngine';
import { formatTimer } from './stage';
import { Icon } from '../ui/Icons';

const PRESETS = [5, 10, 20, 30, 45];
const QUICK = ['Wrap up', '5 minutes left', 'Please come up', 'Mic is off'];


const command = (payload: { type: 'stage_timer'; payload: { action: TimerAction } } | { type: 'set_stage_message'; payload: { text: string } }) => invoke('engine_command', { command: payload });
const timer = (action: TimerAction) => command({ type: 'stage_timer', payload: { action } });

export default function StageControls({ stage, onError }: { stage?: StageState; onError: (message: string) => void }) {
  const [now, setNow] = useState(0);
  const [minutes, setMinutes] = useState('30');
  const [message, setMessage] = useState('');
  const running = stage?.timer?.running_since_ms != null;
  useEffect(() => {
    if (!running) return;
    const first = window.setTimeout(() => setNow(Date.now()), 0);
    const tick = window.setInterval(() => setNow(Date.now()), 250);
    return () => { window.clearTimeout(first); window.clearInterval(tick); };
  }, [running]);
  const act = (work: Promise<unknown>) => void work.catch(e => onError(String(e)));
  const remaining = stage?.timer ? (running && !now ? stage.timer.remaining_ms : timerRemaining(stage.timer, now)) : null;
  const tone = remaining == null ? '' : remaining <= 0 ? ' is-over' : remaining <= 120_000 ? ' is-near' : '';
  const send = (text: string) => act(command({ type: 'set_stage_message', payload: { text } }));
  return <section className="output-panel stage-panel" aria-label="Stage">
    <header className="output-bar"><h2>Stage</h2><small>Shown on the confidence screen</small></header>
    <div className="stage-timer">
      <output className={`stage-clock tabular${tone}`} aria-label="Stage timer">{remaining == null ? '–:––' : formatTimer(remaining)}</output>
      <div className="stage-buttons">
        {!stage?.timer && <>
          {PRESETS.map(p => <button key={p} onClick={() => act(timer({ type: 'start', duration_ms: p * 60_000 }))}>{p}m</button>)}
          <span className="unit-input"><input aria-label="Timer minutes" inputMode="numeric" value={minutes} onChange={e => setMinutes(e.target.value.replace(/\D/g, ''))}/>m</span>
          <button className="primary-action" disabled={!Number(minutes)} onClick={() => act(timer({ type: 'start', duration_ms: Number(minutes) * 60_000 }))}><Icon name="timer"/>Start</button>
        </>}
        {stage?.timer && <>
          <button onClick={() => act(timer(running ? { type: 'pause' } : { type: 'resume' }))}><Icon name={running ? 'pause' : 'play'}/>{running ? 'Pause' : 'Resume'}</button>
          <button aria-label="One minute less" onClick={() => act(timer({ type: 'adjust', delta_ms: -60_000 }))}>−1m</button>
          <button aria-label="One minute more" onClick={() => act(timer({ type: 'adjust', delta_ms: 60_000 }))}>+1m</button>
          <button onClick={() => act(timer({ type: 'clear' }))}><Icon name="close"/>Clear</button>
        </>}
      </div>
    </div>
    <form className="stage-message" onSubmit={e => { e.preventDefault(); if (message.trim()) { send(message); setMessage(''); } }}>
      {stage?.message ? <p className="stage-showing" role="status"><Icon name="message"/><span>{stage.message}</span><button type="button" onClick={() => send('')}>Clear</button></p>
        : <div className="stage-quick">{QUICK.map(q => <button type="button" key={q} onClick={() => send(q)}>{q}</button>)}</div>}
      <div className="stage-message-row"><input aria-label="Message for the stage" placeholder="Message for the stage…" value={message} onChange={e => setMessage(e.target.value)}/><button type="submit" disabled={!message.trim()}>Send</button></div>
    </form>
  </section>;
}
