import { invoke } from '@tauri-apps/api/core';
import { fetch } from '@tauri-apps/plugin-http';

export async function translateSentenceOllama(sentence: string): Promise<string> {
    try {
        const prompt = `Translate the following Japanese game script text into Korean. 
Rules:
1. Maintain the meaning and nuance of the original Japanese.
2. Do not explain the translation, just output the exact Korean translation.
3. Keep proper nouns and context consistent if possible.
4. Handle colloquial game dialogue markers and expressions naturally:
   - A standalone or ending small tsu (e.g. "っ", "っ！", "…っ") represents a gasp, groan, breath, or abrupt cutoff of speech. Do not ignore it or translate it literally (like "tsu"). Translate it into a natural Korean expression of exclamation, sigh, or cutoff (e.g., "앗!", "윽!", "읏…", "흡!", or simply "…!").
   - Conversational starters or particles such as "だって" (datte) or "だって…" indicate a defensive, pleading, or explaining tone. Translate them to colloquial Korean starters like "그치만...", "하지만...", "그야..." instead of literal dictionary translations.

Original text: "${sentence}"`;

        const ollamaModelName = localStorage.getItem('ollamaModelName') || 'gemma4:e4b';
        const response = await fetch("http://127.0.0.1:11434/api/generate", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({
                model: ollamaModelName,
                prompt: prompt,
                options: {
                    num_ctx: 1024,
                    num_predict: 256,
                    temperature: 0.0
                },
                stream: false
            })
        });

        if (!response.ok) {
            throw new Error(`Ollama 통신 실패: ${response.status} ${response.statusText}`);
        }

        const result = await response.json();
        const responseText: string = result.response || "";
        return responseText.trim().replace(/^["'](.*)["']$/, '$1'); // Remove surrounding quotes if present
    } catch (e: any) {
        console.error("Translation error:", e);
        return `Translation Failed: ${e?.message || JSON.stringify(e) || e}`;
    }
}
