import { invoke } from '@tauri-apps/api/core';

export async function translateSentenceOllama(sentence: string): Promise<string> {
    try {
        const prompt = `Translate the following Japanese game script text into Korean. 
Rules:
1. Maintain the meaning and nuance of the original Japanese.
2. Do not explain the translation, just output the exact Korean translation.
3. Keep proper nouns and context consistent if possible.

Original text: "${sentence}"`;

        const ollamaModelName = localStorage.getItem('ollamaModelName') || 'gemma4:e4b';
        const response = await fetch("http://127.0.0.1:11434/api/generate", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({
                model: ollamaModelName,
                prompt: prompt,
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
