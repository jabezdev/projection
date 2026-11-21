import type { CSSProperties } from 'react';
import { isShader, isVideoPath, shaderStyle, useOutputConfig, withOpacity } from '../outputs/preview';
import { useMediaUrl } from '../library/useMediaUrl';
import { useVideoPoster } from '../library/useVideoPoster';
import { isTauri } from '../api/tauri';
import { mediaSource } from '../library/media';

/** A background path (image, video, `shader:<name>` or none) filling its box. */
export function Background({ path, fit }: { path: string; fit?: 'cover' | 'contain' }) {
  const config = useOutputConfig();
  if (!path) return null;
  if (isShader(path)) return <span className="preview-bg is-shader" style={shaderStyle(path.slice(7), config)}/>;
  return isVideoPath(path) ? <VideoStill path={path} fit={fit ?? config.backgroundFit}/> : <ImageFill path={path} fit={fit ?? config.backgroundFit}/>;
}
function ImageFill({ path, fit }: { path: string; fit: 'cover' | 'contain' }) {
  const url = useMediaUrl(path);
  return url ? <img className="preview-bg" src={url} alt="" style={{ objectFit: fit }} draggable={false}/> : null;
}
/** The desktop app shows a cached poster; the browser demo plays the clip muted. */
function VideoStill({ path, fit }: { path: string; fit: 'cover' | 'contain' }) {
  const poster = useVideoPoster(path);
  if (!isTauri()) return <video className="preview-bg" src={mediaSource(path)} muted autoPlay loop playsInline style={{ objectFit: fit }}/>;
  return poster ? <img className="preview-bg" src={poster} alt="" style={{ objectFit: fit }} draggable={false}/> : null;
}

/**
 * One lyric slide laid out like the native projection renderer: sizes are
 * shares of the screen width, the text box spans the margins, and shadow,
 * outline and glow use the same scale.
 */
export function SlidePreview({ text, background, className = '' }: { text: string; background: string; className?: string }) {
  const c = useOutputConfig();
  const px = (value: number) => `${value / 16}cqw`;
  const shadows = [
    c.textShadow && `${px(c.shadowOffsetX)} ${px(c.shadowOffsetY)} ${px(c.shadowSize)} ${withOpacity(c.shadowColor, c.shadowOpacity)}`,
    c.textGlow && c.glowSize > 0 && `0 0 ${px(c.glowSize)} ${withOpacity(c.glowColor, c.glowOpacity)}`,
  ].filter(Boolean).join(', ');
  const area: CSSProperties = { inset: `${c.marginY * 9 / 16}cqw ${c.marginX}cqw`, justifyContent: c.vertical === 'top' ? 'flex-start' : c.vertical === 'center' ? 'center' : 'flex-end' };
  const box: CSSProperties = { background: withOpacity(c.boxColor, c.boxOpacity), border: c.border > 0 ? `${px(c.border)} solid ${c.borderColor}` : undefined, padding: '0.4cqw 1cqw' };
  const type: CSSProperties = {
    fontFamily: `${JSON.stringify(c.font)}, sans-serif`, fontSize: `${c.size}cqw`, fontWeight: c.weight, fontStyle: c.italic ? 'italic' : 'normal',
    lineHeight: c.lineHeight, letterSpacing: px(c.letterSpacing), color: c.color, textAlign: c.align, textTransform: c.caps ? 'uppercase' : 'none',
    textShadow: shadows || 'none',
    WebkitTextStroke: c.textOutline && c.outlineWidth > 0 ? `${px(c.outlineWidth * 2)} ${c.outlineColor}` : undefined, paintOrder: 'stroke fill',
  };
  return <div className={`slide-preview ${className}`}>
    <Background path={background}/>
    {text && <div className="slide-area" style={area}><div style={box}><p style={type}>{text}</p></div></div>}
  </div>;
}
