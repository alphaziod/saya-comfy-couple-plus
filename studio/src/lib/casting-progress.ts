export function castingTimestamp(value: unknown): number | null {
  if(typeof value!=="string"||!value)return null;
  const old=value.match(/^SystemTime \{ tv_sec: (\d+), tv_nsec: (\d+) \}$/);
  const timestamp=old?Number(old[1])*1000+Number(old[2])/1e6:Date.parse(value);
  return Number.isFinite(timestamp)?timestamp:null;
}
export function castingDate(value:unknown):string {
  const timestamp=castingTimestamp(value);
  return timestamp===null?'—':new Date(timestamp).toLocaleString();
}
