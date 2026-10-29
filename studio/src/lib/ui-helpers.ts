// Pure UI helpers shared by components and tested directly under Node.

export function extensionFor(path: string, to: string): string {
  return path.replace(/\.(png|jpe?g)$/i, (match: string) => {
    const png = /png/i.test(match);
    if (to === 'png') return png ? match : '.png';
    return png ? '.jpg' : match;
  });
}
export function extensionMismatch(path: string, to: string): boolean {
  return to === 'png' ? /\.jpe?g$/i.test(path) : /\.png$/i.test(path);
}

// One LoRA per line "name:weight" → [{name, weightModel, weightClip}] (shape expected by ablab.rs).
export function parseLoras(text: string): { name: string; weightModel: number; weightClip: number }[] {
  const result: { name: string; weightModel: number; weightClip: number }[] = [];
  text.split(/\r?\n/).forEach((raw: string, index: number) => {
    const line = raw.trim();
    if (!line) return;
    const cut = line.lastIndexOf(':');
    const name = cut < 0 ? '' : line.slice(0, cut).trim();
    const weightText = cut < 0 ? '' : line.slice(cut + 1).trim();
    const weight = Number(weightText);
    if (!name || !weightText || !Number.isFinite(weight)) throw new Error(`LoRA ligne ${index + 1} : format « nom:poids » attendu (ex. detail_tweaker:0.6).`);
    result.push({ name, weightModel: weight, weightClip: weight });
  });
  return result;
}
