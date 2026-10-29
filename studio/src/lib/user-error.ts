import { t } from './translate-now.ts';
import { ENGINE_ERROR_KEYS } from './engine-error-keys.ts';

const known = new Set(ENGINE_ERROR_KEYS);
// Core errors are French source keys; a variable detail may follow after the first " : ".
function translateEngineError(text:string):string {
 const message=text.trim();
 if(known.has(message))return t(message);
 const cut=message.indexOf(' : ');
 if(cut>0){const prefix=message.slice(0,cut);if(known.has(prefix))return `${t(prefix)} : ${message.slice(cut+3)}`;}
 return text;
}
export function userError(error:unknown):string {
 const text=error instanceof Error?error.message:error&&typeof error==='object'&&'message' in error?String(error.message):String(error);
 if(/(?:error sending request|connection refused|connection reset|timed out|connect error)/i.test(text)&&/https?:\/\/(?:127\.0\.0\.1|localhost|\[::1\]):\d+\/(?:queue|prompt|history|object_info|system_stats)/i.test(text))return t("ComfyUI est indisponible. Ouvre-le pour reprendre le suivi.");
 return translateEngineError(text);
}
