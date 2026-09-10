"""Patch aggregation and installation for ComfyUI attention replacements."""

from __future__ import annotations

from typing import Any, Self

import torch


class RegionalAttentionReplacement:
    """Callable patch aggregator installed on ComfyUI attention blocks."""

    def __init__(self: Self, couple_patch: Any) -> Any:
        """Execute the __init__ operation for this module."""
        self.couple_patch = couple_patch
        self.callback = []
        self.kwargs = []
        self.multigpu_kwargs = {}

    def add(self: Self, callback: Any, **kwargs: Any) -> None:
        """Execute the add operation for this module."""
        self.callback.append(callback)
        self.kwargs.append(kwargs)
        self.multigpu_kwargs = {}
        for key, value in kwargs.items():
            setattr(self, key, value)

    def get_multigpu_kwargs(self: Self, device: Any) -> Any:
        """Execute the get_multigpu_kwargs operation for this module."""
        return self.multigpu_kwargs.get(device, self.kwargs)

    def __call__(self: Self, q: Any, k: Any, v: Any, extra_options: Any) -> Any:
        """Execute the __call__ operation for this module."""
        dtype = q.dtype
        out = self.couple_patch(q, k, v, extra_options)
        sigma = (
            extra_options["sigmas"].detach().cpu()[0].item()
            if "sigmas" in extra_options
            else 999999999.9
        )
        device_kwargs = self.get_multigpu_kwargs(q.device)
        for i, callback in enumerate(self.callback):
            kwargs = device_kwargs[i]
            sigma_start = kwargs.get("sigma_start", 999999999.9)
            sigma_end = kwargs.get("sigma_end", -999999999.9)
            if sigma <= sigma_start and sigma >= sigma_end:
                out = out + callback(out, q, k, v, extra_options, **kwargs)
        return out.to(dtype=dtype)

    def to(self: Self, device: Any, *args: Any, **kwargs: Any) -> Any:
        """Execute the to operation for this module."""
        if not isinstance(device, torch.device):
            return self
        if device == "cpu" or device == torch.device("cpu"):
            return self
        if device in self.multigpu_kwargs and len(self.multigpu_kwargs[device]) == len(self.kwargs):
            return self
        new_kwargs = []
        for kwargs_dict in self.kwargs:
            new_dict = kwargs_dict.copy()
            for key, value in list(new_dict.items()):
                if key == "ipadapter" and hasattr(value, "create_multigpu_clone"):
                    value.create_multigpu_clone(device)
                elif isinstance(value, torch.Tensor):
                    new_dict[key] = value.to(device)
            new_kwargs.append(new_dict)
        self.multigpu_kwargs[device] = new_kwargs
        return self


def install_attention_replacement(model: Any, patch: Any, key: Any) -> None:
    """Install or extend one regional attention replacement on a cloned model."""
    to = model.model_options["transformer_options"]
    if "patches_replace" not in to:
        to["patches_replace"] = {}
    if "attn2" not in to["patches_replace"]:
        to["patches_replace"]["attn2"] = {}
    to["patches_replace"]["attn2"][key] = RegionalAttentionReplacement(patch)
