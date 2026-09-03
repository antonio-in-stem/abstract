package com.abstractlang.runtime;

/** Thrown when a bundle cannot be opened or a value has an unexpected shape. */
public class AbstractDataException extends RuntimeException {

    private static final long serialVersionUID = 1L;

    public AbstractDataException(String message) {
        super(message);
    }

    public AbstractDataException(String message, Throwable cause) {
        super(message, cause);
    }
}
