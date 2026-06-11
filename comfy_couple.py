from nodes import MAX_RESOLUTION, ConditioningCombine, ConditioningConcat, ConditioningSetMask
from comfy_extras.nodes_mask import MaskComposite, SolidMask

from .attention_couple import AttentionCouple


class ComfyCouple:

    @classmethod
    def INPUT_TYPES(cls):
        return {
            "required": {
                "model": ("MODEL",),
                "main_positive": ("CONDITIONING",),
                "person_1_positive": ("CONDITIONING",),
                "person_2_positive": ("CONDITIONING",),
                "negative": ("CONDITIONING",),
                "orientation": (["horizontal", "vertical"],),
                "center": ("FLOAT", {"default": 0.5, "min": 0, "max": 1.0, "step": 0.01}),
                "width": ("INT", {"default": 512, "min": 16, "max": MAX_RESOLUTION, "step": 8}),
                "height": ("INT", {"default": 512, "min": 16, "max": MAX_RESOLUTION, "step": 8}),
                "use_couple_attention": ("BOOLEAN", {"default": True}),
            },
            "optional": {
                "support_model_1": ("MODEL",),
                "support_model_2": ("MODEL",),
            }
        }

    RETURN_TYPES = (
        "MODEL",
        "CONDITIONING",
        "CONDITIONING",
        "CONDITIONING",
        "CONDITIONING",
        "CONDITIONING",
        "CONDITIONING",
        "MASK",
        "MASK",
        "MODEL",
        "MODEL",
    )

    RETURN_NAMES = (
        "model",
        "full_positive",
        "negative",
        "main_positive",
        "person_1_positive",
        "person_2_positive",
        "duo_positive",
        "mask_positive_1",
        "mask_positive_2",
        "support_model_1",
        "support_model_2",
    )

    FUNCTION = "process"
    CATEGORY = "loaders"

    def process(
            self,
            model,
            main_positive,
            person_1_positive,
            person_2_positive,
            negative,
            orientation,
            center,
            width,
            height,
            use_couple_attention,
            support_model_1=None,
            support_model_2=None,
    ):
        solid_mask_zero = SolidMask().solid(0.0, width, height)[0]
        solid_mask_full = SolidMask().solid(1.0, width, height)[0]

        person_1_context = ConditioningConcat().concat(
            main_positive,
            person_1_positive,
        )[0]

        # SAFE SOLO MODE:
        # use_couple_attention OFF:
        # - Person 1 uses the full image
        # - Person 2 is ignored
        # - AttentionCouple is disabled
        # - mask_positive_1 is full screen
        # - mask_positive_2 is empty
        if not use_couple_attention:
            return (
                model,
                person_1_context,
                negative,
                main_positive,
                person_1_positive,
                person_2_positive,
                person_1_positive,
                solid_mask_full,
                solid_mask_zero,
                support_model_1,
                support_model_2,
            )

        mask_rect_first_x = None
        mask_rect_first_y = None
        mask_rect_first_width = None
        mask_rect_first_height = None

        mask_rect_second_x = None
        mask_rect_second_y = None
        mask_rect_second_width = None
        mask_rect_second_height = None

        if orientation == "horizontal":
            width_first = int(width * center)

            mask_rect_first_x = width_first
            mask_rect_first_y = 0
            mask_rect_first_width = width - width_first
            mask_rect_first_height = height

            mask_rect_second_x = 0
            mask_rect_second_y = 0
            mask_rect_second_width = width_first
            mask_rect_second_height = height

        elif orientation == "vertical":
            height_first = int(height * center)

            mask_rect_first_x = 0
            mask_rect_first_y = height_first
            mask_rect_first_width = width
            mask_rect_first_height = height - height_first

            mask_rect_second_x = 0
            mask_rect_second_y = 0
            mask_rect_second_width = width
            mask_rect_second_height = height_first

        solid_mask_first = SolidMask().solid(1.0, mask_rect_first_width, mask_rect_first_height)[0]
        solid_mask_second = SolidMask().solid(1.0, mask_rect_second_width, mask_rect_second_height)[0]

        mask_composite_first = MaskComposite().combine(
            solid_mask_zero,
            solid_mask_first,
            mask_rect_first_x,
            mask_rect_first_y,
            "add",
        )[0]

        mask_composite_second = MaskComposite().combine(
            solid_mask_zero,
            solid_mask_second,
            mask_rect_second_x,
            mask_rect_second_y,
            "add",
        )[0]

        # Same convention as the original node:
        # mask_composite_second = mask_positive_1
        # mask_composite_first  = mask_positive_2

        person_2_context = ConditioningConcat().concat(
            main_positive,
            person_2_positive,
        )[0]

        conditioning_mask_person_1 = ConditioningSetMask().append(
            person_1_context,
            mask_composite_second,
            "default",
            1.0,
        )[0]

        conditioning_mask_person_2 = ConditioningSetMask().append(
            person_2_context,
            mask_composite_first,
            "default",
            1.0,
        )[0]

        positive_combined = ConditioningCombine().combine(
            conditioning_mask_person_1,
            conditioning_mask_person_2,
        )[0]

        duo_positive = ConditioningConcat().concat(
            person_1_positive,
            person_2_positive,
        )[0]

        couple_model, couple_positive, couple_negative = AttentionCouple().attention_couple(
            model,
            positive_combined,
            negative,
            "Attention",
        )

        if support_model_1 is not None:
            support_model_1 = AttentionCouple().attention_couple(
                support_model_1,
                positive_combined,
                negative,
                "Attention",
            )[0]

        if support_model_2 is not None:
            support_model_2 = AttentionCouple().attention_couple(
                support_model_2,
                positive_combined,
                negative,
                "Attention",
            )[0]

        return (
            couple_model,
            couple_positive,
            couple_negative,
            main_positive,
            person_1_positive,
            person_2_positive,
            duo_positive,
            mask_composite_second,
            mask_composite_first,
            support_model_1,
            support_model_2,
        )


NODE_CLASS_MAPPINGS = {
    "Comfy Couple": ComfyCouple
}

NODE_DISPLAY_NAME_MAPPINGS = {
    "Comfy Couple": "Comfy Couple",
}
