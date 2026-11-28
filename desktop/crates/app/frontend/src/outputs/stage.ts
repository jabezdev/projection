export const formatTimer = (ms: number) => {
  const over = ms < 0;
  const seconds = Math.ceil(Math.abs(ms) / 1000);
  const h = Math.floor(seconds / 3600), m = Math.floor(seconds / 60) % 60, s = seconds % 60;
  const body = h ? `${h}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}` : `${m}:${String(s).padStart(2, '0')}`;
  return over ? `+${body}` : body;
};
