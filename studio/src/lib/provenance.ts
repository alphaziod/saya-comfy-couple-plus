export interface Origin { kind:'recipe'|'scene'|'ab_cell'|'module'|'manual'; id:string|null; label:string; version:number|null; at:string }
export interface AbCell { campaign:string; campaignName:string; cell:string; position:number }
export interface ProvOutput { asset:string; path:string|null; nodeId:string|null; outputIndex:number|null; kind:string }
export interface JobProvenance { job:string; state:string; seed:string; createdAt:string; workflow:{id:string;name:string|null}; origin:Origin|null; abCell:AbCell|null; pipeline?:{run:string;step:string;pipeline:string}|null; outputs:ProvOutput[]; path:unknown[]; branchCount:number }
export interface AssetProvenance { asset:string; jobs:JobProvenance[] }
export const originKindLabel:Record<string,string>={recipe:'recette',scene:'scène',ab_cell:'cellule A/B',module:'module',manual:'saisie manuelle'};
export function originText(o:Origin|null):string{
  if(!o)return 'origine non enregistrée';
  const kind=originKindLabel[o.kind]??o.kind;
  return `${kind}${o.label?` · ${o.label}`:''}${o.version!==null&&o.version!==undefined?` · v${o.version}`:''}`;
}
export const UNKNOWN_ORIGIN='origine inconnue : image importée ou antérieure au suivi';
