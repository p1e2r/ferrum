// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors
package io.github.p1e2r.ferrum

import kotlin.math.abs
import kotlin.math.acos
import kotlin.math.asin
import kotlin.math.atan
import kotlin.math.cbrt
import kotlin.math.ceil
import kotlin.math.cos
import kotlin.math.exp
import kotlin.math.floor
import kotlin.math.ln
import kotlin.math.log10
import kotlin.math.log2
import kotlin.math.pow
import kotlin.math.round
import kotlin.math.sin
import kotlin.math.sqrt
import kotlin.math.tan

/**
 * The address-bar calculator, the same as the desktop app's (src/calc.rs):
 * `12*(3+4)`, `2^10`, `sqrt(2)`, `15% of 80`, `200 + 10%`, `2pi`, `3x4`.
 * Text is only a sum when it has an operator or a function, so plain numbers
 * and words are still searches.
 */
object Calc {
    private val FUNCTIONS = setOf("sqrt", "cbrt", "abs", "sin", "cos", "tan", "asin", "acos", "atan", "ln", "log", "log2", "exp", "round", "floor", "ceil")

    /** The answer, formatted for display, if `input` is a sum. */
    fun answer(input: String): String? {
        val s = input.trim().trimEnd('=').trim()
        if (s.isEmpty() || s.length > 200 || !looksLikeSum(s)) return null
        val v = evaluate(s) ?: return null
        return if (v.isFinite()) format(v) else null
    }

    private fun looksLikeSum(s: String): Boolean {
        val lower = s.lowercase()
        if (lower.contains("://") || lower.startsWith("www.")) return false
        if (lower.contains(' ') && lower.split(Regex("\\s+")).any { w -> w.length > 5 && w.all { it.isLetter() } && w !in FUNCTIONS && w != "of" }) return false
        val hasDigit = s.any { it.isDigit() } || lower.contains("pi") || s.contains('π')
        val implicit = listOf("pi", "π", "(").any { t ->
            var i = lower.indexOf(t)
            var found = false
            while (i >= 0) {
                if (i > 0 && lower[i - 1].isDigit()) found = true
                i = lower.indexOf(t, i + 1)
            }
            found
        }
        val chars = s.toCharArray()
        val timesX = chars.indices.any { chars[it] == 'x' && isTimesX(chars, it) }
        val hasOp = s.drop(1).any { it in "+-*/^×÷·%" } || s.contains('√') || implicit || timesX || FUNCTIONS.any { lower.contains("$it(") }
        val dateLike = s.all { it.isDigit() || it == '-' } && s.count { it == '-' } >= 2
        return hasDigit && hasOp && !dateLike
    }

    private fun isTimesX(chars: CharArray, i: Int): Boolean {
        val before = (i - 1 downTo 0).map { chars[it] }.firstOrNull { it != ' ' }
        val after = (i + 1 until chars.size).map { chars[it] }.firstOrNull { it != ' ' }
        return (before != null && (before.isDigit() || before == ')')) && (after != null && (after.isDigit() || after == '('))
    }

    private sealed class Tok {
        data class Num(val v: Double) : Tok()
        data class Op(val c: Char) : Tok()
        object Open : Tok()
        object Close : Tok()
        data class Name(val n: String) : Tok()
        object Of : Tok()
    }

    private fun tokenize(s: String): List<Tok>? {
        val out = ArrayList<Tok>()
        val c = s.toCharArray()
        var i = 0
        while (i < c.size) {
            val ch = c[i]
            when {
                ch == ' ' || ch == '\t' -> i++
                ch.isDigit() || ch == '.' || ch == ',' -> {
                    val start = i
                    while (i < c.size && (c[i].isDigit() || c[i] == '.' || c[i] == ',' || c[i] == '_')) i++
                    if (i + 1 < c.size && (c[i] == 'e' || c[i] == 'E') &&
                        (c[i + 1].isDigit() || ((c[i + 1] == '-' || c[i + 1] == '+') && i + 2 < c.size && c[i + 2].isDigit()))
                    ) {
                        i += 2
                        while (i < c.size && c[i].isDigit()) i++
                    }
                    val text = String(c, start, i - start).filter { it != ',' && it != '_' }
                    out.add(Tok.Num(text.toDoubleOrNull() ?: return null))
                }
                ch in "+-*/^%" -> {
                    if (ch == '*' && i + 1 < c.size && c[i + 1] == '*') {
                        out.add(Tok.Op('^')); i += 2
                    } else {
                        out.add(Tok.Op(ch)); i++
                    }
                }
                ch == '×' || ch == '·' || (ch == 'x' && isTimesX(c, i)) -> { out.add(Tok.Op('*')); i++ }
                ch == '÷' -> { out.add(Tok.Op('/')); i++ }
                ch == '(' || ch == '[' -> { out.add(Tok.Open); i++ }
                ch == ')' || ch == ']' -> { out.add(Tok.Close); i++ }
                ch == '√' -> { out.add(Tok.Name("sqrt")); i++ }
                ch == 'π' -> { out.add(Tok.Name("pi")); i++ }
                ch.isLetter() -> {
                    val start = i
                    while (i < c.size && c[i].isLetterOrDigit()) i++
                    val word = String(c, start, i - start).lowercase()
                    out.add(if (word == "of") Tok.Of else Tok.Name(word))
                }
                else -> return null
            }
        }
        return out
    }

    fun evaluate(s: String): Double? {
        val tokens = tokenize(s) ?: return null
        val p = Parser(tokens)
        val v = p.expr() ?: return null
        return if (p.pos == tokens.size) v else null
    }

    private class Parser(val t: List<Tok>) {
        var pos = 0
        var lastWasPercent = false
        fun peek(): Tok? = t.getOrNull(pos)

        fun expr(): Double? {
            var v = term() ?: return null
            while (true) {
                val op = (peek() as? Tok.Op)?.c ?: break
                if (op != '+' && op != '-') break
                pos++
                var rhs = term() ?: return null
                if (lastWasPercent) rhs *= v
                lastWasPercent = false
                v = if (op == '+') v + rhs else v - rhs
            }
            return v
        }

        fun term(): Double? {
            var v = unary() ?: return null
            while (true) {
                when (val n = peek()) {
                    is Tok.Op -> when (n.c) {
                        '*' -> { pos++; v *= unary() ?: return null }
                        '/' -> {
                            pos++
                            val d = unary() ?: return null
                            if (d == 0.0) return null
                            v /= d
                        }
                        else -> return v
                    }
                    is Tok.Of -> { pos++; v *= unary() ?: return null; lastWasPercent = false }
                    is Tok.Open, is Tok.Name, is Tok.Num -> v *= unary() ?: return null
                    else -> return v
                }
            }
        }

        fun unary(): Double? {
            val n = peek()
            if (n is Tok.Op && n.c == '-') { pos++; return unary()?.let { -it } }
            if (n is Tok.Op && n.c == '+') { pos++; return unary() }
            return power()
        }

        fun power(): Double? {
            val base = postfix() ?: return null
            val n = peek()
            if (n is Tok.Op && n.c == '^') {
                pos++
                val e = unary() ?: return null
                return base.pow(e)
            }
            return base
        }

        fun postfix(): Double? {
            val v = atom() ?: return null
            val n = peek()
            if (n is Tok.Op && n.c == '%') {
                pos++
                lastWasPercent = true
                return v / 100.0
            }
            lastWasPercent = false
            return v
        }

        fun atom(): Double? {
            val tok = t.getOrNull(pos) ?: return null
            pos++
            return when (tok) {
                is Tok.Num -> tok.v
                is Tok.Open -> {
                    val v = expr() ?: return null
                    when (peek()) {
                        is Tok.Close -> { pos++; v }
                        null -> v
                        else -> null
                    }
                }
                is Tok.Name -> when (tok.n) {
                    "pi" -> Math.PI
                    "e" -> Math.E
                    "tau" -> Math.PI * 2
                    else -> {
                        val a = power() ?: return null
                        when (tok.n) {
                            "sqrt" -> sqrt(a)
                            "cbrt" -> cbrt(a)
                            "abs" -> abs(a)
                            "sin" -> sin(a)
                            "cos" -> cos(a)
                            "tan" -> tan(a)
                            "asin" -> asin(a)
                            "acos" -> acos(a)
                            "atan" -> atan(a)
                            "ln" -> ln(a)
                            "log" -> log10(a)
                            "log2" -> log2(a)
                            "exp" -> exp(a)
                            "round" -> round(a)
                            "floor" -> floor(a)
                            "ceil" -> ceil(a)
                            else -> null
                        }
                    }
                }
                else -> null
            }
        }
    }

    /** Up to 10 significant digits, thousands grouped, no trailing zeros. */
    fun format(v: Double): String {
        if (v == 0.0) return "0"
        val a = abs(v)
        if (a < 1e-6 || a >= 1e15) {
            val s = String.format(java.util.Locale.ROOT, "%.6e", v)
            val (m, e) = s.split('e')
            val exp = e.toInt()
            return m.trimEnd('0').trimEnd('.') + "e" + exp
        }
        val digits = (10 - floor(log10(a)).toInt() - 1).coerceIn(0, 10)
        var s = String.format(java.util.Locale.ROOT, "%.${digits}f", v)
        if (s.contains('.')) s = s.trimEnd('0').trimEnd('.')
        val neg = s.startsWith('-')
        val body = s.removePrefix("-")
        val int = body.substringBefore('.')
        val frac = if (body.contains('.')) body.substringAfter('.') else null
        val grouped = StringBuilder()
        int.forEachIndexed { n, ch ->
            if (n > 0 && (int.length - n) % 3 == 0) grouped.append(',')
            grouped.append(ch)
        }
        return (if (neg) "-" else "") + grouped + (frac?.let { ".$it" } ?: "")
    }
}
