#!/usr/bin/env python3
"""CLIP ViT-H/14 CPU sidecar for Saya Studio visual search.

Modes:
- serve: persistent JSON-lines worker, model loaded once, exits after
  --idle-seconds without request (memory released). This is what the app uses.
- one-shot: --paths/--output for benchmarks/tests.

No network, no download, no GPU. Python/model/script are explicit paths.
"""
import argparse, json, os, select, sys, time
from pathlib import Path


def load_model(model_path, threads):
    import torch
    from safetensors.torch import load_file
    from transformers import CLIPVisionConfig, CLIPVisionModel
    torch.set_num_threads(max(1, threads))
    torch.set_grad_enabled(False)
    state = load_file(model_path)
    state = {k.removeprefix('vision_model.'): v for k, v in state.items()
             if k != 'vision_model.embeddings.position_ids'}
    projection = state.pop('visual_projection.weight', None)
    if projection is not None:
        projection = projection.float()
    cfg = CLIPVisionConfig(hidden_size=1280, intermediate_size=5120,
                           num_hidden_layers=32, num_attention_heads=16,
                           image_size=224, patch_size=14, projection_dim=1024)
    model = CLIPVisionModel(cfg).eval()
    missing, _ = model.load_state_dict(state, strict=False)
    if missing:
        raise SystemExit(f'missing weights: {missing[:3]}')
    mean = torch.tensor([0.48145466, 0.4578275, 0.40821073]).view(3, 1, 1)
    std = torch.tensor([0.26862954, 0.26130258, 0.27577711]).view(3, 1, 1)
    return model, projection, mean, std


def preprocess(path, torch, Image, mean, std):
    image = Image.open(path).convert('RGB')
    w, h = image.size
    scale = 224 / min(w, h)
    image = image.resize((max(224, round(w * scale)), max(224, round(h * scale))),
                         Image.Resampling.BICUBIC)
    w, h = image.size
    left, top = (w - 224) // 2, (h - 224) // 2
    image = image.crop((left, top, left + 224, top + 224))
    x = torch.tensor(list(image.getdata()), dtype=torch.float32)
    x = x.view(224, 224, 3).permute(2, 0, 1) / 255.0
    return (x - mean) / std


def embed(paths, model, projection, mean, std, batch_size):
    import torch
    from PIL import Image
    vectors, errors = {}, {}
    preprocess_ms = infer_ms = 0.0
    for start in range(0, len(paths), batch_size):
        batch_paths = paths[start:start + batch_size]
        tensors = []
        for path in batch_paths:
            try:
                t = time.perf_counter()
                tensors.append((preprocess(path, torch, Image, mean, std), path,
                                (time.perf_counter() - t) * 1000))
            except Exception as error:
                errors[str(path)] = str(error)
        if not tensors:
            continue
        preprocess_ms += sum(item[2] for item in tensors)
        t = time.perf_counter()
        with torch.no_grad():
            output = model(pixel_values=torch.stack([item[0] for item in tensors])).pooler_output
            if projection is not None:
                output = output.float() @ projection.T
            output = torch.nn.functional.normalize(output, dim=1)
        infer_ms += (time.perf_counter() - t) * 1000
        for index, (_, path, _) in enumerate(tensors):
            vectors[str(path)] = output[index].tolist()
    return vectors, errors, preprocess_ms, infer_ms


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--model', required=True)
    ap.add_argument('--serve', action='store_true')
    ap.add_argument('--paths')
    ap.add_argument('--output')
    ap.add_argument('--threads', type=int, default=4)
    ap.add_argument('--batch-size', type=int, default=16)
    ap.add_argument('--idle-seconds', type=float, default=120)
    a = ap.parse_args()
    batch_size = max(1, min(32, a.batch_size))
    t0 = time.perf_counter()
    model, projection, mean, std = load_model(a.model, a.threads)
    load_ms = (time.perf_counter() - t0) * 1000
    if not a.serve:
        if not a.paths or not a.output:
            raise SystemExit('--paths/--output requis hors --serve')
        paths = json.loads(Path(a.paths).read_text())
        vectors, errors, pre_ms, infer_ms = embed(paths, model, projection, mean, std, batch_size)
        Path(a.output).write_text(json.dumps({
            'model': 'clip-vit-h-14-comfyui', 'version': 1, 'dim': 1024,
            'vectors': vectors, 'errors': errors,
            'timings': {'loadMs': round(load_ms, 1), 'preprocessMs': round(pre_ms, 1),
                        'inferMs': round(infer_ms, 1), 'batchSize': batch_size,
                        'count': len(paths)},
        }))
        print(json.dumps({'embedded': len(vectors), 'failed': len(errors),
                          'loadMs': round(load_ms, 1), 'preprocessMs': round(pre_ms, 1),
                          'inferMs': round(infer_ms, 1), 'batchSize': batch_size}))
        return
    first = True
    while True:
        ready, _, _ = select.select([sys.stdin], [], [], max(1.0, a.idle_seconds))
        if not ready:
            return
        line = sys.stdin.readline()
        if not line:
            return
        try:
            request = json.loads(line)
        except Exception:
            print(json.dumps({'error': 'bad request'}), flush=True)
            continue
        if request.get('shutdown'):
            return
        paths = request.get('paths') or []
        vectors, errors, pre_ms, infer_ms = embed(paths, model, projection, mean, std, batch_size)
        response = {
            'model': 'clip-vit-h-14-comfyui', 'version': 1, 'dim': 1024,
            'vectors': vectors, 'errors': errors,
            'timings': {'loadMs': round(load_ms, 1) if first else 0.0,
                        'preprocessMs': round(pre_ms, 1), 'inferMs': round(infer_ms, 1),
                        'batchSize': batch_size, 'count': len(paths)},
        }
        first = False
        print(json.dumps(response), flush=True)

if __name__ == '__main__':
    main()
