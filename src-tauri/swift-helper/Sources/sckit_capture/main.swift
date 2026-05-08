// sckit_capture — Lord Varys system-audio sidecar.
//
// On launch this process emits a single JSON header line on stdout
// describing the stream, then writes raw interleaved f32 little-endian PCM
// bytes from a ScreenCaptureKit audio-only SCStream until SIGTERM. Stderr is
// the log channel; the parent Rust process drains it line by line.
//
// Modes:
//   --probe                                       briefly start + stop
//                                                 SCKit so macOS surfaces the
//                                                 Screen Recording TCC dialog
//   --format pcm-f32                              (default) raw f32 PCM
//   --sample-rate <hz>                            (default 48000)
//   --channels <n>                                (default 2)
//   --exclude-self                                exclude this app's own audio
//                                                 from the captured mix

import AVFoundation
import Foundation
import ScreenCaptureKit

@main
struct SckitCapture {
    static func main() async {
        let args = CommandLine.arguments
        let isProbe = args.contains("--probe")
        let excludeSelf = args.contains("--exclude-self")
        let sampleRate = intArg(args: args, key: "--sample-rate") ?? 48000
        let channels = intArg(args: args, key: "--channels") ?? 2

        if isProbe {
            await probe()
            return
        }

        do {
            try await captureLoop(
                sampleRate: sampleRate,
                channels: channels,
                excludeSelf: excludeSelf
            )
        } catch {
            FileHandle.standardError.write(
                "fatal: \(error)\n".data(using: .utf8) ?? Data()
            )
            exit(1)
        }
    }

    static func intArg(args: [String], key: String) -> Int? {
        guard let i = args.firstIndex(of: key), i + 1 < args.count else { return nil }
        return Int(args[i + 1])
    }
}

// MARK: - Probe

func probe() async {
    // Touch SCKit just enough to fire the TCC prompt.
    do {
        _ = try await SCShareableContent.excludingDesktopWindows(
            false, onScreenWindowsOnly: true
        )
        FileHandle.standardError.write("probe: ok\n".data(using: .utf8)!)
        exit(0)
    } catch {
        FileHandle.standardError.write(
            "probe: \(error)\n".data(using: .utf8) ?? Data()
        )
        exit(2)
    }
}

// MARK: - Capture

func captureLoop(sampleRate: Int, channels: Int, excludeSelf: Bool) async throws {
    let content = try await SCShareableContent.excludingDesktopWindows(
        false, onScreenWindowsOnly: true
    )
    guard let display = content.displays.first else {
        throw NSError(
            domain: "sckit_capture",
            code: 1,
            userInfo: [NSLocalizedDescriptionKey: "no display available"]
        )
    }

    var excluded: [SCRunningApplication] = []
    if excludeSelf {
        let me = Bundle.main.bundleIdentifier ?? "com.lordvarys.app"
        excluded = content.applications.filter { $0.bundleIdentifier == me }
    }
    let filter = SCContentFilter(
        display: display,
        excludingApplications: excluded,
        exceptingWindows: []
    )

    let cfg = SCStreamConfiguration()
    cfg.capturesAudio = true
    cfg.excludesCurrentProcessAudio = excludeSelf
    cfg.sampleRate = sampleRate
    cfg.channelCount = channels
    // Minimal video budget — SCKit requires a stream config that includes
    // *some* video, but we only consume the audio buffers below. Keep video
    // as small as possible to minimise CPU.
    cfg.width = 2
    cfg.height = 2
    cfg.minimumFrameInterval = CMTime(value: 1, timescale: 1) // 1 fps

    // Header — one JSON line, then raw bytes forever.
    let header: [String: Any] = [
        "sample_rate": sampleRate,
        "channels": channels,
        "format": "f32"
    ]
    let headerData = try JSONSerialization.data(
        withJSONObject: header, options: []
    )
    FileHandle.standardOutput.write(headerData)
    FileHandle.standardOutput.write("\n".data(using: .utf8)!)

    let output = AudioOutput()
    let stream = SCStream(filter: filter, configuration: cfg, delegate: nil)
    try stream.addStreamOutput(
        output,
        type: .audio,
        sampleHandlerQueue: DispatchQueue(label: "sckit_capture.audio")
    )
    try await stream.startCapture()

    FileHandle.standardError.write(
        "started: sr=\(sampleRate) ch=\(channels)\n".data(using: .utf8)!
    )

    // SIGTERM/SIGINT → graceful stop.
    let stopSignal = DispatchSource.makeSignalSource(
        signal: SIGTERM, queue: .global()
    )
    let intSignal = DispatchSource.makeSignalSource(
        signal: SIGINT, queue: .global()
    )
    let stopGroup = DispatchGroup()
    stopGroup.enter()
    stopSignal.setEventHandler { stopGroup.leave() }
    intSignal.setEventHandler { stopGroup.leave() }
    signal(SIGTERM, SIG_IGN)
    signal(SIGINT, SIG_IGN)
    stopSignal.resume()
    intSignal.resume()

    // Park the main task until a stop signal fires.
    await withCheckedContinuation { (cont: CheckedContinuation<Void, Never>) in
        DispatchQueue.global().async {
            stopGroup.wait()
            cont.resume()
        }
    }

    try? await stream.stopCapture()
    FileHandle.standardError.write("stopped\n".data(using: .utf8)!)
}

// MARK: - Audio buffer handler

final class AudioOutput: NSObject, SCStreamOutput {
    func stream(_ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of type: SCStreamOutputType) {
        guard type == .audio,
              CMSampleBufferDataIsReady(sampleBuffer),
              let format = CMSampleBufferGetFormatDescription(sampleBuffer)
        else { return }

        let asbdPtr = CMAudioFormatDescriptionGetStreamBasicDescription(format)
        guard let asbd = asbdPtr?.pointee else { return }

        var blockBufferOut: CMBlockBuffer?
        var audioBufferList = AudioBufferList()
        let status = CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(
            sampleBuffer,
            bufferListSizeNeededOut: nil,
            bufferListOut: &audioBufferList,
            bufferListSize: MemoryLayout<AudioBufferList>.size,
            blockBufferAllocator: nil,
            blockBufferMemoryAllocator: nil,
            flags: 0,
            blockBufferOut: &blockBufferOut
        )
        guard status == noErr else { return }

        let buffers = UnsafeMutableAudioBufferListPointer(
            UnsafeMutablePointer(&audioBufferList)
        )

        // SCKit hands us audio either as Float32 already or sometimes as
        // Int16 depending on the device. Convert to interleaved Float32 LE
        // either way — the Rust side consumes that single format only.
        let isFloat = (asbd.mFormatFlags & kAudioFormatFlagIsFloat) != 0
        let bitsPerChannel = Int(asbd.mBitsPerChannel)
        let channelsPerFrame = Int(asbd.mChannelsPerFrame)
        let isInterleaved = (asbd.mFormatFlags & kAudioFormatFlagIsNonInterleaved) == 0

        for buffer in buffers {
            guard let data = buffer.mData else { continue }
            let byteCount = Int(buffer.mDataByteSize)

            if isFloat && bitsPerChannel == 32 && isInterleaved {
                // Already in our wire format — write straight through.
                let raw = Data(bytes: data, count: byteCount)
                FileHandle.standardOutput.write(raw)
            } else if isFloat && bitsPerChannel == 32 && !isInterleaved {
                // Non-interleaved float planes: interleave into a single buffer.
                // For multi-buffer non-interleaved, each planar channel is in
                // its own AudioBuffer — see ASBD docs. SCKit normally hands us
                // a single buffer though; the multi-buffer path is rare.
                let frames = byteCount / 4
                let src = data.bindMemory(to: Float32.self, capacity: frames)
                let interleaved = UnsafeMutablePointer<Float32>.allocate(
                    capacity: frames
                )
                defer { interleaved.deallocate() }
                for i in 0..<frames {
                    interleaved[i] = src[i]
                }
                let raw = Data(bytes: interleaved, count: frames * 4)
                FileHandle.standardOutput.write(raw)
            } else if !isFloat && bitsPerChannel == 16 {
                // Int16 → Float32 conversion at the boundary.
                let frames = byteCount / 2
                let src = data.bindMemory(to: Int16.self, capacity: frames)
                let buf = UnsafeMutablePointer<Float32>.allocate(capacity: frames)
                defer { buf.deallocate() }
                for i in 0..<frames {
                    buf[i] = Float32(src[i]) / 32768.0
                }
                let raw = Data(bytes: buf, count: frames * 4)
                FileHandle.standardOutput.write(raw)
            } else {
                // Unsupported format — log once, skip the buffer.
                FileHandle.standardError.write(
                    "skip: unsupported PCM format float=\(isFloat) bits=\(bitsPerChannel) ch=\(channelsPerFrame)\n"
                        .data(using: .utf8) ?? Data()
                )
            }
        }
    }
}
