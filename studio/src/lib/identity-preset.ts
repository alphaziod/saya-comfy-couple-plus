export function identityText(character: {identity:Record<string,unknown>}|undefined, variant?:{promptBlocks:Record<string,string>}):string {
  if (!character) throw new Error('Sélectionnez un personnage.');
  const text=variant?variant.promptBlocks.P1:character.identity.identity_prompt??character.identity.identityPrompt;
  if(typeof text!=='string'||!text.trim()) throw new Error('Identité texte absente : la variante doit fournir sa section P1.');
  return text;
}
export function applyIdentityPreset<T extends {enabled:boolean;section:string|null;valueType:string;value:string}>(fields:T[],section:'P1'|'P2',text:string):T[] {
  const targets=fields.filter(f=>f.enabled&&f.section===section);
  if(targets.length!==1||targets[0].valueType!=='string') throw new Error(`${section} exige une seule entrée texte explicitement mappée.`);
  return fields.map(f=>f===targets[0]?{...f,value:text}:f);
}
export function composePrompt(before:string,after:string,append:boolean):string {
  return append&&before ? before+(after?'\n'+after:'') : after;
}
