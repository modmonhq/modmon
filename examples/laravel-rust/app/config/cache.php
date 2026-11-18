<?php

return [
    'default' => env('CACHE_STORE', 'redis'),
    'stores' => [
        'redis' => ['driver' => 'redis', 'connection' => 'cache'],
        'array' => ['driver' => 'array', 'serialize' => false],
    ],
    'prefix' => 'laravel_rust_cache',
];
