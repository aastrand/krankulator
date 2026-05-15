#!/usr/bin/env python3
"""
Audio comparison tool for NES emulator APU testing.

Compares emulator WAV output against reference hardware recordings.
Produces JSON diagnostics and PNG comparison images.

Usage:
    python analyze_audio.py emulator.wav --reference reference.mp3 [--report-dir ./reports]
"""

import argparse
import json
import os
import sys

import librosa
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
from scipy import signal

SR = 44100


def load_audio(path):
    y, _ = librosa.load(path, sr=SR, mono=True)
    peak = np.max(np.abs(y))
    if peak > 0:
        y = y / peak
    return y


def find_first_onset(y, hop=512, threshold_ratio=0.15):
    """Find the sample index of the first energy onset (start of first beep)."""
    rms = librosa.feature.rms(y=y, frame_length=2048, hop_length=hop)[0]
    threshold = np.max(rms) * threshold_ratio
    for i, val in enumerate(rms):
        if val > threshold:
            return max(0, i * hop - hop)
    return 0


def time_align(emu, ref):
    """Align on first beep onset, trim to one complete pass (length of reference)."""
    emu_onset = find_first_onset(emu)
    ref_onset = find_first_onset(ref)

    emu = emu[emu_onset:]
    ref = ref[ref_onset:]

    # Trim to reference length (one complete pass)
    trim_len = min(len(emu), len(ref))
    offset = emu_onset - ref_onset
    return emu[:trim_len], ref[:trim_len], offset


def detect_segments(y, hop=512):
    """Detect beep-test-beep structure. Returns labeled segments including gaps."""
    rms = librosa.feature.rms(y=y, frame_length=2048, hop_length=hop)[0]
    times = librosa.frames_to_time(np.arange(len(rms)), sr=SR, hop_length=hop)
    duration = len(y) / SR

    threshold = np.max(rms) * 0.15
    is_active = rms > threshold

    active_regions = []
    in_segment = False
    seg_start = 0

    for i, active in enumerate(is_active):
        if active and not in_segment:
            seg_start = i
            in_segment = True
        elif not active and in_segment:
            active_regions.append((float(times[seg_start]), float(times[i])))
            in_segment = False

    if in_segment:
        active_regions.append((float(times[seg_start]), float(times[min(len(times) - 1, len(is_active) - 1)])))

    # Identify beep1 (first short active region) and beep2 (last short active region)
    short_regions = [(s, e) for s, e in active_regions if e - s < 1.0]

    labeled = []
    if len(short_regions) >= 2:
        beep1_start, beep1_end = short_regions[0]
        beep2_start, beep2_end = short_regions[-1]

        labeled.append({"name": "beep1", "start_sec": round(beep1_start, 3), "end_sec": round(beep1_end, 3)})
        labeled.append({"name": "test_region", "start_sec": round(beep1_end, 3), "end_sec": round(beep2_start, 3)})
        labeled.append({"name": "beep2", "start_sec": round(beep2_start, 3), "end_sec": round(beep2_end, 3)})
    elif len(short_regions) == 1:
        beep_start, beep_end = short_regions[0]
        labeled.append({"name": "beep1", "start_sec": round(beep_start, 3), "end_sec": round(beep_end, 3)})
        labeled.append({"name": "test_region", "start_sec": round(beep_end, 3), "end_sec": round(duration, 3)})
    else:
        labeled.append({"name": "full", "start_sec": 0.0, "end_sec": round(duration, 3)})

    return labeled


def spectral_bands(y, n_bands=24, max_freq=16000):
    """Compute log-spaced spectral band magnitudes (20Hz to max_freq)."""
    n_fft = 4096
    S = np.abs(librosa.stft(y, n_fft=n_fft))
    freqs = librosa.fft_frequencies(sr=SR, n_fft=n_fft)

    band_edges = np.logspace(np.log10(20), np.log10(max_freq), n_bands + 1)
    band_mags = []

    for i in range(n_bands):
        mask = (freqs >= band_edges[i]) & (freqs < band_edges[i + 1])
        if np.any(mask):
            band_mags.append(float(np.mean(S[mask, :])))
        else:
            band_mags.append(1e-10)

    return np.array(band_mags), band_edges


def analyze_segment(emu_seg, ref_seg):
    """Compute per-segment comparison metrics."""
    result = {}

    emu_rms = float(np.sqrt(np.mean(emu_seg ** 2)))
    ref_rms = float(np.sqrt(np.mean(ref_seg ** 2)))
    result["emulator_rms"] = round(emu_rms, 6)
    result["reference_rms"] = round(ref_rms, 6)

    # Waveform cross-correlation
    if len(emu_seg) > 0 and np.std(emu_seg) > 1e-8 and np.std(ref_seg) > 1e-8:
        corr = np.corrcoef(emu_seg, ref_seg)[0, 1]
        result["waveform_correlation"] = round(float(corr), 4)
    else:
        result["waveform_correlation"] = 0.0

    # Spectral comparison
    emu_bands, edges = spectral_bands(emu_seg)
    ref_bands, _ = spectral_bands(ref_seg)

    emu_db = 20 * np.log10(emu_bands + 1e-10)
    ref_db = 20 * np.log10(ref_bands + 1e-10)
    db_diff = emu_db - ref_db
    result["spectral_bands_db_diff"] = [round(float(d), 2) for d in db_diff]
    result["max_spectral_deviation_db"] = round(float(np.max(np.abs(db_diff))), 2)

    # Band center frequencies for reference
    band_centers = [(edges[i] + edges[i + 1]) / 2 for i in range(len(edges) - 1)]
    result["band_center_frequencies_hz"] = [round(float(f), 1) for f in band_centers]

    # Spectral similarity (cosine)
    dot = np.dot(emu_bands, ref_bands)
    norm = np.linalg.norm(emu_bands) * np.linalg.norm(ref_bands)
    result["spectral_similarity"] = round(float(dot / norm) if norm > 0 else 0.0, 4)

    # Envelope correlation
    env_emu = np.abs(signal.hilbert(emu_seg))
    env_ref = np.abs(signal.hilbert(ref_seg))
    win = min(2048, len(env_emu) // 4)
    if win > 0:
        kernel = np.ones(win) / win
        env_emu_s = np.convolve(env_emu, kernel, mode="same")
        env_ref_s = np.convolve(env_ref, kernel, mode="same")
        if np.std(env_emu_s) > 1e-8 and np.std(env_ref_s) > 1e-8:
            result["envelope_correlation"] = round(float(np.corrcoef(env_emu_s, env_ref_s)[0, 1]), 4)
        else:
            result["envelope_correlation"] = 0.0
    else:
        result["envelope_correlation"] = 0.0

    # Dominant error frequencies
    diff = emu_seg - ref_seg
    diff_rms = float(np.sqrt(np.mean(diff ** 2)))
    result["rms_error"] = round(diff_rms, 6)

    if diff_rms > 0.001:
        S_diff = np.abs(np.fft.rfft(diff))
        freqs_diff = np.fft.rfftfreq(len(diff), 1.0 / SR)
        top_idx = np.argsort(S_diff)[-5:][::-1]
        result["dominant_error_frequencies_hz"] = [round(float(freqs_diff[i]), 1) for i in top_idx if S_diff[i] > np.max(S_diff) * 0.3]

    return result


def generate_diagnosis(overall, segments):
    """Generate human/AI-readable diagnosis string from metrics."""
    parts = []

    env = overall.get("envelope_correlation", 0)
    if env > 0.95:
        parts.append(f"Excellent envelope match (r={env:.3f}).")
    elif env > 0.8:
        parts.append(f"Good envelope match (r={env:.3f}).")
    elif env > 0.5:
        parts.append(f"Moderate envelope match (r={env:.3f}) — volume shape differs.")
    else:
        parts.append(f"Poor envelope match (r={env:.3f}) — major volume shape differences.")

    test_region = next((s for s in segments if s["name"] == "test_region"), None)
    if test_region:
        emu_rms = test_region.get("emulator_rms", 0)
        ref_rms = test_region.get("reference_rms", 0)
        ratio = emu_rms / ref_rms if ref_rms > 1e-6 else float('inf')
        if ratio < 1.2:
            parts.append(f"Test region: excellent match (emu={emu_rms:.4f}, ref={ref_rms:.4f}, ratio={ratio:.2f}x).")
        elif ratio < 2.0:
            parts.append(f"Test region: good match (emu={emu_rms:.4f}, ref={ref_rms:.4f}, ratio={ratio:.2f}x).")
        else:
            parts.append(f"Test region: residual too high (emu={emu_rms:.4f}, ref={ref_rms:.4f}, ratio={ratio:.2f}x).")

    for seg in segments:
        err_freqs = seg.get("dominant_error_frequencies_hz", [])
        if err_freqs and seg.get("rms_error", 0) > 0.01:
            freq_str = ", ".join(f"{f:.0f}Hz" for f in err_freqs[:3])
            parts.append(f"Segment '{seg['name']}': error energy concentrated at {freq_str}.")

        seg_dev = seg.get("max_spectral_deviation_db", 0)
        if seg_dev > 6:
            bands = seg.get("spectral_bands_db_diff", [])
            centers = seg.get("band_center_frequencies_hz", [])
            worst_idx = int(np.argmax(np.abs(bands))) if bands else -1
            if worst_idx >= 0 and worst_idx < len(centers):
                parts.append(
                    f"Segment '{seg['name']}': worst band at ~{centers[worst_idx]:.0f}Hz "
                    f"({bands[worst_idx]:+.1f}dB vs reference)."
                )

    return " ".join(parts) if parts else "No significant differences detected."


def plot_spectrograms(emu, ref, name, report_dir):
    fig, (ax1, ax2) = plt.subplots(1, 2, figsize=(14, 5))
    fig.suptitle(f"{name} — Spectrogram Comparison")

    for ax, y, title in [(ax1, emu, "Emulator"), (ax2, ref, "Reference")]:
        S = librosa.amplitude_to_db(np.abs(librosa.stft(y)), ref=np.max)
        librosa.display.specshow(S, sr=SR, x_axis="time", y_axis="log", ax=ax)
        ax.set_title(title)

    plt.tight_layout()
    path = os.path.join(report_dir, f"{name}_spectrogram.png")
    fig.savefig(path, dpi=150)
    plt.close(fig)
    return path


def plot_waveforms(emu, ref, name, report_dir):
    fig, (ax1, ax2) = plt.subplots(2, 1, figsize=(14, 6))
    fig.suptitle(f"{name} — Waveform Comparison")

    t = np.arange(len(emu)) / SR
    ax1.plot(t, ref, color="red", alpha=0.6, linewidth=0.5, label="Reference")
    ax1.plot(t, emu, color="blue", alpha=0.6, linewidth=0.5, label="Emulator")
    ax1.set_ylabel("Amplitude")
    ax1.legend()
    ax1.set_title("Overlay")

    diff = emu - ref
    ax2.plot(t, diff, color="green", linewidth=0.5)
    ax2.set_ylabel("Difference")
    ax2.set_xlabel("Time (s)")
    ax2.set_title(f"Difference (RMS={np.sqrt(np.mean(diff**2)):.4f})")

    plt.tight_layout()
    path = os.path.join(report_dir, f"{name}_waveform.png")
    fig.savefig(path, dpi=150)
    plt.close(fig)
    return path


def plot_spectrum(emu, ref, name, report_dir):
    fig, (ax1, ax2) = plt.subplots(2, 1, figsize=(14, 7))
    fig.suptitle(f"{name} — Spectrum Comparison")

    emu_bands, edges = spectral_bands(emu)
    ref_bands, _ = spectral_bands(ref)
    centers = [(edges[i] + edges[i + 1]) / 2 for i in range(len(edges) - 1)]

    emu_db = 20 * np.log10(emu_bands + 1e-10)
    ref_db = 20 * np.log10(ref_bands + 1e-10)

    ax1.semilogx(centers, ref_db, "r-o", markersize=3, label="Reference")
    ax1.semilogx(centers, emu_db, "b-o", markersize=3, label="Emulator")
    ax1.set_ylabel("Magnitude (dB)")
    ax1.legend()
    ax1.set_title("Spectral Magnitude")
    ax1.grid(True, alpha=0.3)

    db_diff = emu_db - ref_db
    colors = ["green" if abs(d) < 3 else "orange" if abs(d) < 6 else "red" for d in db_diff]
    ax2.bar(range(len(db_diff)), db_diff, color=colors)
    ax2.axhline(y=0, color="black", linewidth=0.5)
    ax2.axhline(y=3, color="orange", linewidth=0.5, linestyle="--")
    ax2.axhline(y=-3, color="orange", linewidth=0.5, linestyle="--")
    ax2.set_ylabel("dB Difference (emu - ref)")
    ax2.set_xlabel("Band index (low → high frequency)")
    ax2.set_title("Per-Band Deviation")

    plt.tight_layout()
    path = os.path.join(report_dir, f"{name}_spectrum.png")
    fig.savefig(path, dpi=150)
    plt.close(fig)
    return path


def plot_envelope(emu, ref, name, report_dir):
    fig, ax = plt.subplots(1, 1, figsize=(14, 4))
    fig.suptitle(f"{name} — Envelope Comparison")

    hop = 512
    emu_rms = librosa.feature.rms(y=emu, frame_length=2048, hop_length=hop)[0]
    ref_rms = librosa.feature.rms(y=ref, frame_length=2048, hop_length=hop)[0]
    min_len = min(len(emu_rms), len(ref_rms))
    t = librosa.frames_to_time(np.arange(min_len), sr=SR, hop_length=hop)

    ax.plot(t, ref_rms[:min_len], "r-", alpha=0.7, label="Reference")
    ax.plot(t, emu_rms[:min_len], "b-", alpha=0.7, label="Emulator")
    ax.set_ylabel("RMS Energy")
    ax.set_xlabel("Time (s)")
    ax.legend()
    ax.grid(True, alpha=0.3)

    plt.tight_layout()
    path = os.path.join(report_dir, f"{name}_envelope.png")
    fig.savefig(path, dpi=150)
    plt.close(fig)
    return path


def main():
    parser = argparse.ArgumentParser(description="NES APU audio comparison")
    parser.add_argument("emulator_wav", help="Path to emulator WAV file")
    parser.add_argument("--reference", required=True, help="Path to reference audio (MP3/WAV)")
    parser.add_argument("--report-dir", default=None, help="Directory for PNG reports")
    parser.add_argument("--tolerance-envelope", type=float, default=0.95,
                        help="Minimum envelope correlation to pass (default: 0.95)")
    parser.add_argument("--tolerance-test-region-rms-ratio", type=float, default=2.0,
                        help="Maximum ratio of emulator/reference test_region RMS (default: 2.0)")
    args = parser.parse_args()

    name = os.path.splitext(os.path.basename(args.emulator_wav))[0]
    report_dir = args.report_dir or os.path.dirname(args.emulator_wav) or "."
    os.makedirs(report_dir, exist_ok=True)

    emu_raw = load_audio(args.emulator_wav)
    ref_raw = load_audio(args.reference)

    emu, ref, offset = time_align(emu_raw, ref_raw)

    segments = detect_segments(ref)

    # If no segments detected, treat entire signal as one region
    if not segments:
        segments = [{"name": "full", "start_sec": 0.0, "end_sec": round(len(ref) / SR, 3)}]

    seg_results = []
    for seg in segments:
        start = int(seg["start_sec"] * SR)
        end = int(seg["end_sec"] * SR)
        start = max(0, min(start, len(emu)))
        end = max(start, min(end, len(emu)))
        if end - start < 100:
            continue
        r = analyze_segment(emu[start:end], ref[start:end])
        r["name"] = seg["name"]
        r["start_sec"] = seg["start_sec"]
        r["end_sec"] = seg["end_sec"]
        seg_results.append(r)

    # Overall metrics
    overall = analyze_segment(emu, ref)

    # Pass/fail: envelope correlation for overall shape, test_region RMS ratio for cancellation
    test_region = next((s for s in seg_results if s["name"] == "test_region"), None)
    envelope_ok = overall.get("envelope_correlation", 0) >= args.tolerance_envelope
    if test_region is not None and test_region["reference_rms"] > 1e-6:
        rms_ratio = test_region["emulator_rms"] / test_region["reference_rms"]
        test_rms_ok = rms_ratio <= args.tolerance_test_region_rms_ratio
        passes = envelope_ok and test_rms_ok
    else:
        passes = envelope_ok
    overall["pass"] = passes

    diagnosis = generate_diagnosis(overall, seg_results)
    overall["diagnosis_summary"] = diagnosis

    # Generate plots
    image_paths = []
    image_paths.append(plot_spectrograms(emu, ref, name, report_dir))
    image_paths.append(plot_waveforms(emu, ref, name, report_dir))
    image_paths.append(plot_spectrum(emu, ref, name, report_dir))
    image_paths.append(plot_envelope(emu, ref, name, report_dir))

    result = {
        "overall": overall,
        "segments": seg_results,
        "alignment_offset_samples": offset,
        "emulator_duration_sec": round(len(emu_raw) / SR, 3),
        "reference_duration_sec": round(len(ref_raw) / SR, 3),
        "aligned_duration_sec": round(len(emu) / SR, 3),
        "images": image_paths,
    }

    json.dump(result, sys.stdout, indent=2)
    print()

    # Also print image paths to stderr for test harness
    for p in image_paths:
        print(f"IMAGE: {p}", file=sys.stderr)

    sys.exit(0 if passes else 1)


if __name__ == "__main__":
    main()
