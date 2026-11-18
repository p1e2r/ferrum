// SPDX-License-Identifier: MIT
package io.github.p1e2r.ferrum

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class CalcTest {
    @Test
    fun sums() {
        val cases = mapOf(
            "1+1" to "2", "12*(3+4)" to "84", "2^10" to "1,024", "2**3" to "8", "10/4" to "2.5",
            "-3 + 5" to "2", "2^3^2" to "512", "-2^2" to "-4", "sqrt(16)" to "4", "√2" to "1.414213562",
            "2pi" to "6.283185307", "3(4+1)" to "15", "15% of 80" to "12", "200 + 10%" to "220",
            "50%" to "0.5", "3 × 4 ÷ 2" to "6", "3x4" to "12", "1,000 * 3" to "3,000", "1e3 + 1" to "1,001",
            "0.1+0.2" to "0.3", "1/3" to "0.3333333333", "(1+2" to "3", "5*5=" to "25", "log(1000)" to "3",
            "2^100" to "1.267651e30",
        )
        for ((input, expected) in cases) assertEquals(input, expected, Calc.answer(input))
    }

    @Test
    fun notSums() {
        for (s in listOf("42", "rust", "github.com", "https://a.com/x-y", "c++ tutorial", "2026-11-12", "1/0", "best laptops 2026", "covid-19 news", "x", "e", "pi", "")) {
            assertNull(s, Calc.answer(s))
        }
    }

    @Test
    fun addresses() {
        assertEquals("https://example.com/", Location.toUrl("example.com", SearchEngine.DuckDuckGo))
        assertEquals("http://localhost:8080/", Location.toUrl("localhost:8080", SearchEngine.DuckDuckGo))
        assertTrue(Location.toUrl("rust programming", SearchEngine.Google).startsWith("https://www.google.com/search?q=rust"))
        assertTrue(Location.looksLikeAddress("192.168.1.1"))
        assertFalse(Location.looksLikeAddress("hello world"))
    }

    @Test
    fun blocking() {
        assertEquals("example.co.uk", AdBlock.baseDomain("news.example.co.uk"))
        assertEquals("example.com", AdBlock.baseDomain("a.b.example.com"))
    }

    @Test
    fun versions() {
        assertTrue(Updater.newer("1.10.0", "1.9.3"))
        assertTrue(Updater.newer("1.2.0", "1.1.0-debug"))
        assertFalse(Updater.newer("1.1.0", "1.1.0"))
        assertFalse(Updater.newer("1.0.9", "1.1.0"))
    }
}
