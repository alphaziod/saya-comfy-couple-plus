"""Phase checkpoint services: candidate -> validated transactions, manifests,
path safety, widget parsing, concurrency."""

import json
import threading

import torch

from harness import Check, load_pack, sample_v2_imprint_json


def _services(tmp_path):
    load_pack()
    from saya_couple.src.services import image_phases as services

    services.output_directory = lambda: tmp_path
    return services


def _image():
    return torch.rand(1, 32, 32, 3)


def test_checkpoint_roundtrip(tmp_path):
    services = _services(tmp_path)
    c = Check("checkpoint_roundtrip")
    root = "test/checkpoints"

    c.raises(ValueError, lambda: services.load_validated_source(1, "none", root),
             "phase 1 has no previous checkpoint")

    services.save_candidate(
        images=_image(), phase=1, detailer="none", checkpoint_root=root,
        source_path="", seed=42, positive_prompt="pos", negative_prompt="neg",
        models=[], vaes=[], samplers={}, imprint_json=sample_v2_imprint_json(),
    )
    paths = services.checkpoint_paths(1, root)
    c.ok(paths.candidate_image.is_file(), "candidate PNG written")
    c.ok(paths.candidate_manifest.is_file(), "candidate manifest written")

    validated = services.promote_candidate(1, "none", root)
    c.eq(validated["status"], "validated", "promoted status")
    c.ok(paths.validated_image.is_file(), "validated PNG written")
    c.ok(not paths.candidate_image.exists(), "candidate consumed")

    image, manifest, path = services.load_validated_source(2, "none", root)
    c.eq(tuple(image.shape), (1, 32, 32, 3), "loaded image shape")
    c.eq(manifest["seed"], 42, "seed preserved")
    c.eq(manifest["positive_prompt"], "pos", "prompt preserved")
    c.eq(manifest["transaction_uuid"], validated["transaction_uuid"], "same transaction")
    c.ok(str(path).endswith("phase_1_base.png"), "validated path")
    return c.report()


def test_phase5_restarts_from_phase4(tmp_path):
    services = _services(tmp_path)
    c = Check("phase5_restarts_from_phase4")
    root = "test/checkpoints"
    for phase in (4, 5):
        services.save_candidate(
            images=_image(), phase=phase, detailer="none", checkpoint_root=root,
            source_path="", seed=phase, positive_prompt="", negative_prompt="",
            models=[], vaes=[], samplers={},
        )
        services.promote_candidate(phase, "none", root)
    _img, manifest, path = services.load_validated_source(5, "face", root)
    c.eq(manifest["phase"], 4, "phase 5 loads phase 4, never an older phase 5")
    c.ok("phase_4" in str(path), "phase 4 file")
    return c.report()


def test_manifest_validation(tmp_path):
    services = _services(tmp_path)
    c = Check("manifest_validation")
    root = "test/checkpoints"
    paths = services.checkpoint_paths(2, root)
    paths.directory.mkdir(parents=True, exist_ok=True)

    c.raises(FileNotFoundError, lambda: services.read_manifest(paths.validated_manifest),
             "missing manifest raises FileNotFoundError")

    paths.validated_manifest.write_text("not json", encoding="utf-8")
    c.raises(ValueError, lambda: services.read_manifest(paths.validated_manifest),
             "malformed JSON refused")

    paths.validated_manifest.write_text(json.dumps([1, 2]), encoding="utf-8")
    c.raises(ValueError, lambda: services.read_manifest(paths.validated_manifest),
             "non-dict manifest refused")
    return c.report()


def test_path_traversal(tmp_path):
    services = _services(tmp_path)
    c = Check("path_traversal")
    c.raises(ValueError, lambda: services.checkpoint_directory("../escape"),
             ".. refused")
    c.raises(ValueError, lambda: services.checkpoint_directory("/abs/path"),
             "absolute path refused")
    target = services.checkpoint_directory("a//b\\c")
    c.ok(str(target).startswith(str(tmp_path)), "separators normalized inside output")
    return c.report()


def test_parse_json_widget(tmp_path):
    services = _services(tmp_path)
    c = Check("parse_json_widget")
    c.eq(services.parse_json_widget(None, list, "x"), [], "None -> empty list")
    c.eq(services.parse_json_widget("", dict, "x"), {}, "blank -> empty dict")
    c.eq(services.parse_json_widget("[1, 2]", list, "x"), [1, 2], "JSON list parsed")
    c.eq(services.parse_json_widget((1, 2), list, "x"), [1, 2], "tuple normalized")
    c.eq(services.parse_json_widget("plain text", list, "x"), ["plain text"],
         "non-JSON string wrapped, never raises")
    c.eq(services.parse_json_widget(5, dict, "x"), {"value": 5}, "scalar wrapped in dict")
    return c.report()


def test_concurrent_promotion(tmp_path):
    services = _services(tmp_path)
    c = Check("concurrent_promotion")
    root = "test/checkpoints"
    services.save_candidate(
        images=_image(), phase=3, detailer="none", checkpoint_root=root,
        source_path="", seed=7, positive_prompt="", negative_prompt="",
        models=[], vaes=[], samplers={},
    )
    results, errors = [], []

    def promote():
        try:
            results.append(services.promote_candidate(3, "none", root))
        except Exception as error:
            errors.append(error)

    threads = [threading.Thread(target=promote) for _ in range(4)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()

    c.eq(len(results), 1, "exactly one promotion wins")
    c.eq(len(errors), 3, "losers fail cleanly")
    paths = services.checkpoint_paths(3, root)
    c.ok(paths.validated_image.is_file(), "winner's image intact")
    c.ok(paths.validated_manifest.is_file(), "winner's manifest intact")
    # No rollback/temp debris left behind. The global promote.lock file lives
    # in the output root, not in this phase's own checkpoint directory, so it
    # never shows up here.
    leftovers = [p.name for p in paths.directory.iterdir() if p.name.startswith(".")]
    c.eq(leftovers, [], "no temp/rollback debris")
    # The validated pair must load: the loser's rollback must not have corrupted it.
    _img, manifest, _path = services.load_validated_source(4, "none", root)
    c.eq(manifest["seed"], 7, "winner's manifest loads")
    return c.report()




def test_transaction_mismatch_refused(tmp_path):
    """A PNG and sidecar from two different promotions must not load."""
    services = _services(tmp_path)
    c = Check("transaction_mismatch_refused")
    root = "test/mismatch"  # own root: other tests share this tmpdir
    for seed in (11, 22):
        services.save_candidate(
            images=_image(), phase=5, detailer="none", checkpoint_root=root,
            source_path="", seed=seed, positive_prompt="", negative_prompt="",
            models=[], vaes=[], samplers={},
        )
        services.promote_candidate(5, "none", root)
    paths = services.checkpoint_paths(5, root)
    # Corrupt the sidecar: claim another transaction while the PNG keeps its own.
    sidecar = json.loads(paths.validated_manifest.read_text())
    sidecar["transaction_uuid"] = "00000000-0000-0000-0000-000000000000"
    paths.validated_manifest.write_text(json.dumps(sidecar), encoding="utf-8")
    c.raises(ValueError, lambda: services.load_validated_source(6, "none", root),
             "mixed-transaction pair refused")
    return c.report()


def test_save_promote_race_regression(tmp_path):
    """The legacy save_candidate -> promote_candidate pair has a window
    between its two separate locks; the atomic save_and_promote_candidate does
    not."""
    services = _services(tmp_path)
    c = Check("save_promote_race_regression")
    root = "test/race"

    # Characterization of the legacy hole: a discard intercalated between the
    # two locked calls makes the promote fail on the now-missing candidate.
    services.save_candidate(
        images=_image(), phase=1, detailer="none", checkpoint_root=root,
        source_path="", seed=1, positive_prompt="", negative_prompt="",
        models=[], vaes=[], samplers={},
    )
    services.discard_candidate(1, root)
    c.raises(FileNotFoundError, lambda: services.promote_candidate(1, "none", root),
             "legacy path: promote after intercalated discard fails")

    # The atomic entry point: a discard spamming concurrently can never land
    # between the candidate write and the promotion, so every call must leave
    # a validated pair that matches the manifest it returned.
    stop = threading.Event()
    spam_errors = []

    def discard_spam():
        while not stop.is_set():
            try:
                services.discard_candidate(2, root)
            except Exception as error:  # must never happen
                spam_errors.append(error)

    spammer = threading.Thread(target=discard_spam)
    spammer.start()
    try:
        for iteration in range(10):
            validated, validated_path = services.save_and_promote_candidate(
                images=_image(), phase=2, detailer="none", checkpoint_root=root,
                source_path="", seed=iteration, positive_prompt="",
                negative_prompt="", models=[], vaes=[], samplers={},
            )
            paths = services.checkpoint_paths(2, root)
            on_disk = services.read_manifest(paths.validated_manifest)
            c.eq(validated["status"], "validated", f"iteration {iteration}: promoted")
            c.eq(validated["seed"], iteration, f"iteration {iteration}: seed preserved")
            c.eq(on_disk["transaction_uuid"], validated["transaction_uuid"],
                 f"iteration {iteration}: validated pair is this transaction")
            c.eq(str(validated_path), str(paths.validated_image),
                 f"iteration {iteration}: validated path returned")
            c.ok(not paths.candidate_manifest.exists(),
                 f"iteration {iteration}: candidate consumed")
    finally:
        stop.set()
        spammer.join()
    c.eq(spam_errors, [], "concurrent discard never raised")
    return c.report()


def test_load_validated_source_holds_promote_lock(tmp_path):
    """Reading a validated pair must run under promote_lock, otherwise a
    concurrent /validate promotion can interleave between the manifest read
    and the PNG read and trip the same-transaction check."""
    services = _services(tmp_path)
    c = Check("load_validated_source_holds_promote_lock")
    root = "test/lockcheck"
    services.save_candidate(
        images=_image(), phase=1, detailer="none", checkpoint_root=root,
        source_path="", seed=5, positive_prompt="", negative_prompt="",
        models=[], vaes=[], samplers={}, imprint_json=sample_v2_imprint_json(),
    )
    services.promote_candidate(1, "none", root)

    state = {}

    def load():
        try:
            state["result"] = services.load_validated_source(2, "none", root)
        except Exception as error:
            state["error"] = error

    with services.promote_lock():
        thread = threading.Thread(target=load)
        thread.start()
        thread.join(timeout=2)
        c.ok(thread.is_alive(), "load blocks while promote_lock is held")
    thread.join(timeout=5)
    c.ok(not thread.is_alive(), "load completes once the lock is released")
    c.ok("result" in state, "load succeeded")
    c.eq(state.get("result", (None, {}, None))[1].get("seed"), 5, "loaded seed")
    return c.report()


def test_emit_phase_complete_targets_queuing_client(tmp_path):
    """The phase-complete event goes only to the client that queued the prompt,
    like ComfyUI's own executing/executed events, so another tab never reacts."""
    services = _services(tmp_path)
    c = Check("emit_phase_complete_targets_queuing_client")

    import server

    instance = server.PromptServer.instance
    sent = []
    original_send, original_client = instance.send_sync, getattr(instance, "client_id", None)
    instance.send_sync = lambda event, payload, sid=None: sent.append((event, payload, sid))
    instance.client_id = "tab-A"
    try:
        manifest = {"transaction_uuid": "tx", "result_file": "/x.png"}
        c.ok(services.emit_phase_complete(
            phase=2, next_phase=3, manifest=manifest, unload_report={}), "emit returns True")
        event, payload, sid = sent[-1]
        c.eq(event, "saya_image_phase_complete", "event name unchanged")
        c.eq(sid, "tab-A", "sent to the queuing client only")
        c.eq((payload["phase"], payload["next_phase"], payload["transaction_uuid"]), (2, 3, "tx"), "payload")
    finally:
        instance.send_sync, instance.client_id = original_send, original_client
    return c.report()


def test_invalidate_phase_holds_promote_lock(tmp_path):
    """The Phase 1 redo must not delete checkpoint files in the middle of a
    concurrent promotion: invalidate_phase waits for promote_lock."""
    services = _services(tmp_path)
    c = Check("invalidate_phase_holds_promote_lock")
    root = "test/invalidatelock"
    services.save_candidate(
        images=_image(), phase=1, detailer="none", checkpoint_root=root,
        source_path="", seed=5, positive_prompt="", negative_prompt="",
        models=[], vaes=[], samplers={}, imprint_json=sample_v2_imprint_json(),
    )
    services.promote_candidate(1, "none", root)
    validated = services.checkpoint_paths(1, root).validated_image

    thread = threading.Thread(target=lambda: services.invalidate_phase(1, root))
    with services.promote_lock():
        thread.start()
        thread.join(timeout=2)
        c.ok(thread.is_alive(), "invalidate blocks while promote_lock is held")
        c.ok(validated.exists(), "validated image untouched while the lock is held")
    thread.join(timeout=5)
    c.ok(not thread.is_alive(), "invalidate completes once the lock is released")
    c.ok(not validated.exists(), "validated image removed after the lock is released")
    return c.report()


def run(tmp_path):
    results = []
    for fn in (
        test_checkpoint_roundtrip,
        test_phase5_restarts_from_phase4,
        test_manifest_validation,
        test_path_traversal,
        test_parse_json_widget,
        test_concurrent_promotion,
        test_transaction_mismatch_refused,
        test_save_promote_race_regression,
        test_load_validated_source_holds_promote_lock,
        test_invalidate_phase_holds_promote_lock,
        test_emit_phase_complete_targets_queuing_client,
    ):
        results.append(fn(tmp_path))
    return results
