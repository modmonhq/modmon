<?php

use Illuminate\Http\Client\ConnectionException;
use Illuminate\Support\Facades\Http;
use Illuminate\Support\Facades\Route;

Route::get('/', fn () => response()->json([
    'application' => config('app.name'),
    'environment' => app()->environment(),
    'role' => config('app.role'),
    'endpoints' => [
        'health' => url('/up'),
        'quote' => url('/api/quotes/example-sku'),
    ],
]));

Route::get('/api/quotes/{sku}', function (string $sku) {
    try {
        $response = Http::baseUrl(config('services.pricing.url'))
            ->acceptJson()
            ->timeout(2)
            ->get('/v1/quotes/'.rawurlencode($sku));
    } catch (ConnectionException) {
        return response()->json(['error' => 'pricing_service_unavailable'], 503);
    }

    return response()->json($response->json(), $response->status());
})->where('sku', '[A-Za-z0-9._-]+');
