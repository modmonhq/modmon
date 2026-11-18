<?php

use Illuminate\Http\Request;
use Illuminate\Support\Facades\Mail;
use Illuminate\Support\Facades\Route;

Route::post('/mail-preview', function (Request $request) {
    $to = $request->string('to')->toString();
    if (filter_var($to, FILTER_VALIDATE_EMAIL) === false) {
        return response()->json(['message' => 'A valid to address is required.'], 422);
    }

    try {
        Mail::raw('The Laravel example reached shared Mailpit.', function ($message) use ($to) {
            $message->to($to)->subject('mm Mailpit preview');
        });
    } catch (Throwable $error) {
        report($error);

        return response()->json([
            'message' => app()->hasDebugModeEnabled() ? $error->getMessage() : 'Mail delivery failed.',
        ], 500);
    }

    return response()->json([
        'accepted' => true,
        'mailpit' => env('MAILPIT_UI_URL'),
    ], 202);
});
