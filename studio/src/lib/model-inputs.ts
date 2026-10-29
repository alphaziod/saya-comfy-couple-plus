export function missingModel(input:string, value:string, options:string[]|null):boolean {
  if(!options)return false;
  const modelField=/(?:ckpt|vae|unet|clip|lora|model)_name(?:_\d+)?$/i.test(input)
    || options.some(v=>/\.(?:safetensors|ckpt|gguf|pt|pth|bin)$/i.test(v));
  return modelField&&!options.includes(value);
}
