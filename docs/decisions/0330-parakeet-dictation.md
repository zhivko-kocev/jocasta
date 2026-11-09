# 0330 Parakeet for dictation, no spike

- Status: decided
- Kind: product
- Phase/slice: 4/4.8
- Date: 2026-10-08

## Question
Which local speech model does dictation use?

## Facts
- 0272 ruled a spike between Parakeet TDT 0.6B v3 (sherpa-onnx) and whisper.cpp on 20 of his sentences. Checked 2026-10-08: `sherpa-onnx-node` 1.13.8 on npm; int8 Parakeet models about 600 MB. Parakeet v3 covers about 25 European languages, believed not to include Macedonian (unverified); he dictates in English.
- His words: "i think parakeet fits better in our case, can you suggest also what you really think".

## Options
| Option | Pros | Cons |
|---|---|---|
| Parakeet, English, no spike; ask before download | Leanest; accurate and fast on CPU | No comparison run |
| Run the spike | Measured | Two downloads and his recordings |

## Recommendation
Parakeet, no spike.

## Choice
Parakeet TDT 0.6B v3 int8 through `sherpa-onnx-node`, English; the model download asks first. 0272's spike is not run.

## Reason
"i think parakeet fits better in our case, can you suggest also what you really think"

## Consequences
- Amends 0272.
- `sherpa-onnx-node` is a native addon: npm installs the matching platform package (`sherpa-onnx-win-x64`, `-linux-x64`, `-darwin-arm64`, …) as an optional dependency, and each release target ships its own next to `jarvis.exe`.
- He approved installing `react`, `react-dom`, `babel-plugin-react-compiler`, the React types, `sherpa-onnx-node` and the Parakeet download on 2026-10-08.
- Amended by 0556: Parakeet stays, run by omp (`modelRoles.dictation: local/parakeet-tdt-0.6b-v3`); `sherpa-onnx-node` and Jocasta's recognizer leave the repo.
