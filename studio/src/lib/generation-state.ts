export function phaseRows(run: any) {
  return Array.from({length:run?.phaseCount??6}, (_,i)=>({n:i+1,...run?.phases?.[String(i+1)],state:run?.phases?.[String(i+1)]?.state??'waiting'}));
}
export function runControls(run:any) {
  const phases=phaseRows(run);
  const active=phases.some(p=>['submitted','submitting','queued','running','submission_unknown'].includes(p.state));
  const next=phases.find(p=>p.state!=='succeeded')?.n??7;
  // Stop stays available until the run is finished: phase in progress (interrupted) or between two phases.
  return {stop:active||(!!run&&run.state==='running'),continue:!run?.singleJob&&!!run&&run.state==='running'&&!active&&next>1&&next<=6&&phases[next-1].state==='waiting',restart:!run?.singleJob&&!!run&&!active,next};
}
export function randomSeed() {
  const bytes=new Uint32Array(2);crypto.getRandomValues(bytes);
  return ((BigInt(bytes[0])<<32n|BigInt(bytes[1]))%1125899906842624n).toString();
}
export function resolveSeed(mode:string,value:string,last:string) {
  let seed=value.trim();
  if(mode==='randomize'||seed==='-1')return randomSeed();
  if(mode==='last'){if(!last)throw Error('Aucune seed réellement soumise pour ce workflow.');seed=last;}
  if(mode==='increment'||mode==='decrement'||seed==='-2'||seed==='-3'){
    const increment=mode==='increment'||seed==='-2';
    if(!last)return randomSeed();
    const n=BigInt(last);seed=(increment?n+1n:n>0n?n-1n:0n).toString();
  }
  if(!seed)return '';
  if(!/^\d+$/.test(seed)||BigInt(seed)>18446744073709551615n)throw Error('Seed entière u64 requise (0 à 18446744073709551615).');
  return BigInt(seed).toString();
}
