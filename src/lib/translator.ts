import kuromoji from 'kuromoji';
import { invoke } from '@tauri-apps/api/core';

export interface Token {
    word_id: number;
    word_type: string;
    word_position: number;
    surface_form: string;
    pos: string;
    pos_detail_1: string;
    pos_detail_2: string;
    pos_detail_3: string;
    conjugated_type: string;
    conjugated_form: string;
    basic_form: string;
    reading: string;
    pronunciation: string;
}

let tokenizer: kuromoji.Tokenizer<Token> | null = null;

export function initTokenizer(): Promise<void> {
    return new Promise((resolve, reject) => {
        if (tokenizer) return resolve();
        kuromoji.builder({ dicPath: '/dict' }).build((err, _tokenizer) => {
            if (err) {
                console.error("Failed to build tokenizer", err);
                return reject(err);
            }
            tokenizer = _tokenizer;
            resolve();
        });
    });
}

export function tokenize(text: string): Token[] {
    if (!tokenizer) throw new Error("Tokenizer not initialized");
    return tokenizer.tokenize(text) as Token[];
}

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
