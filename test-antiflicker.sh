#!/bin/bash
# Anti-flicker strategy tester for 0fbuf
# Cycles through all strategies with visual feedback

STRATEGIES=("none" "all" "unmap" "offscreen" "utility" "iconic" "lower" "combo1" "combo2" "combo3")

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  0fbuf Anti-Flicker Strategy Tester"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "This script will test each anti-flicker strategy one by one."
echo "Watch for window flashes during pool pre-warming."
echo ""
echo "Press ENTER to start..."
read

for strategy in "${STRATEGIES[@]}"; do
    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "  Testing strategy: $strategy"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""

    # Kill existing daemon
    echo "[1/3] Stopping existing daemon..."
    pkill 0fbuf
    sleep 1

    # Start daemon with strategy
    echo "[2/3] Starting daemon with strategy: $strategy"
    echo "      Command: 0fbuf --antiflicker $strategy daemon"
    echo ""
    echo "      WATCH FOR FLASHES NOW!"
    echo ""

    0fbuf --antiflicker "$strategy" daemon &
    DAEMON_PID=$!

    # Let it pre-warm
    echo "[3/3] Waiting for pool pre-warming (10 seconds)..."
    sleep 10

    # Kill daemon
    kill $DAEMON_PID 2>/dev/null
    wait $DAEMON_PID 2>/dev/null

    echo ""
    echo "Strategy '$strategy' test complete."
    echo ""
    echo "Did you see window flashes? (y/n/somewhat): "
    read -r response

    case $response in
        y|Y) echo "✗ Flash detected with '$strategy'" ;;
        n|N) echo "✓ NO FLASH with '$strategy' - WINNER!" ;;
        *) echo "~ Partial flash with '$strategy'" ;;
    esac

    echo ""
    echo "Press ENTER to test next strategy (or Ctrl+C to quit)..."
    read
done

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  All strategies tested!"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "Update your config with the best strategy:"
echo "  0fbuf config"
echo "  # Set: antiflicker_strategy = \"your_best_strategy\""
echo ""
